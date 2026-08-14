// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{
    collections::BTreeMap,
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

use poche_capture::{
    CaptureChunkFetchCall, CaptureChunkFetchResult, CaptureChunkUploadCall,
    CaptureDeliveryAcknowledgeCall, CaptureProviderPollCall, CaptureProviderRegistrationReceipt,
    CaptureProviderResponseCall, CaptureProviderUnregisterCall, CaptureProviderUnregisterReceipt,
    CaptureRelayAcknowledgement, CaptureRelayJob, CaptureRelayToken, CaptureRequestRelayReceipt,
    EncryptedCaptureChunk, WrappedCaptureTransferKey,
};
use poche_player_client::{DeviceClientError, DeviceCooperationRequest, DeviceCooperationResult};
use poche_protocol::{
    CaptureProviderAdvertisementWire, CaptureRequestId, CaptureResponseOutcomeWire,
    CaptureResponseWire, CaptureTransferDescriptorWire, CaptureTransferId, DeviceCertificateWire,
    DeviceId,
};
use poche_runtime::{RuntimeDeviceCooperationContext, RuntimeDeviceCooperationHandler};

const PROVIDER_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Default)]
pub(crate) struct CaptureRelay {
    shared: Arc<(Mutex<RelayState>, Condvar)>,
}

#[derive(Default)]
struct RelayState {
    providers: BTreeMap<DeviceId, RelayProvider>,
    jobs: BTreeMap<CaptureRequestId, RelayJobState>,
}

struct RelayProvider {
    token: CaptureRelayToken,
    advertisement: CaptureProviderAdvertisementWire,
}

struct RelayJobState {
    request: poche_protocol::CaptureRequestWire,
    requester_certificate: DeviceCertificateWire,
    provider_device_id: DeviceId,
    delivery_token: CaptureRelayToken,
    response: Option<CaptureResponseWire>,
    wrapped_keys: Vec<WrappedCaptureTransferKey>,
    chunks: BTreeMap<(CaptureTransferId, u32), EncryptedCaptureChunk>,
}

pub(crate) struct RelayCaptureHandler {
    relay: CaptureRelay,
    provider_device_id: DeviceId,
}

impl CaptureRelay {
    pub(crate) fn register(
        &self,
        advertisement: CaptureProviderAdvertisementWire,
    ) -> Result<(CaptureProviderRegistrationReceipt, RelayCaptureHandler), DeviceClientError> {
        advertisement
            .validate()
            .map_err(|_| DeviceClientError::ProtocolViolation)?;
        let token = CaptureRelayToken::random().map_err(|_| DeviceClientError::KeyUnavailable)?;
        let provider_device_id = advertisement.provider_device_id.clone();
        let (lock, _) = &*self.shared;
        let mut state = lock
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        if state
            .providers
            .get(&provider_device_id)
            .is_some_and(|existing| {
                existing.advertisement.advertisement_sequence
                    >= advertisement.advertisement_sequence
            })
        {
            return Err(DeviceClientError::AuthorizationDenied);
        }
        state.providers.insert(
            provider_device_id.clone(),
            RelayProvider {
                token: token.clone(),
                advertisement,
            },
        );
        Ok((
            CaptureProviderRegistrationReceipt {
                provider_device_id: provider_device_id.clone(),
                provider_token: token,
            },
            RelayCaptureHandler {
                relay: self.clone(),
                provider_device_id,
            },
        ))
    }

    pub(crate) fn poll(
        &self,
        call: &CaptureProviderPollCall,
    ) -> Result<Option<CaptureRelayJob>, DeviceClientError> {
        call.provider_token
            .validate()
            .map_err(|_| DeviceClientError::AuthorizationDenied)?;
        let (lock, _) = &*self.shared;
        let state = lock
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        let provider = state
            .providers
            .values()
            .find(|provider| provider.token == call.provider_token)
            .ok_or(DeviceClientError::AuthorizationDenied)?;
        Ok(state
            .jobs
            .values()
            .find(|job| {
                job.provider_device_id == provider.advertisement.provider_device_id
                    && job.response.is_none()
            })
            .map(|job| CaptureRelayJob {
                request: job.request.clone(),
                requester_certificate: job.requester_certificate.clone(),
            }))
    }

    pub(crate) fn unregister(&self, receipt: &CaptureProviderRegistrationReceipt) {
        let (lock, _) = &*self.shared;
        if let Ok(mut state) = lock.lock() {
            let matches = state
                .providers
                .get(&receipt.provider_device_id)
                .is_some_and(|provider| provider.token == receipt.provider_token);
            if matches {
                state.providers.remove(&receipt.provider_device_id);
            }
        }
    }

    pub(crate) fn unregister_call(
        &self,
        call: &CaptureProviderUnregisterCall,
    ) -> Result<CaptureProviderUnregisterReceipt, DeviceClientError> {
        call.provider_token
            .validate()
            .map_err(|_| DeviceClientError::AuthorizationDenied)?;
        let (lock, _) = &*self.shared;
        let mut state = lock
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        let device = provider_for_token(&state, &call.provider_token)?.clone();
        state.providers.remove(&device);
        Ok(CaptureProviderUnregisterReceipt {
            provider_device_id: device,
        })
    }

    pub(crate) fn provide_response(
        &self,
        call: CaptureProviderResponseCall,
    ) -> Result<CaptureRelayAcknowledgement, DeviceClientError> {
        let (lock, changed) = &*self.shared;
        let mut state = lock
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        let provider = provider_for_token(&state, &call.provider_token)?;
        if provider != &call.response.provider_device_id {
            return Err(DeviceClientError::AuthorizationDenied);
        }
        let job = state
            .jobs
            .get_mut(&call.response.request_id)
            .ok_or(DeviceClientError::TransportUnavailable)?;
        validate_response_keys(job, &call.response, &call.wrapped_keys)?;
        if let Some(existing) = &job.response {
            return if existing == &call.response && job.wrapped_keys == call.wrapped_keys {
                Ok(CaptureRelayAcknowledgement {
                    request_id: call.response.request_id,
                    accepted: true,
                })
            } else {
                Err(DeviceClientError::ProtocolViolation)
            };
        }
        let request_id = call.response.request_id.clone();
        job.response = Some(call.response);
        job.wrapped_keys = call.wrapped_keys;
        changed.notify_all();
        Ok(CaptureRelayAcknowledgement {
            request_id,
            accepted: true,
        })
    }

    pub(crate) fn upload_chunk(
        &self,
        call: CaptureChunkUploadCall,
    ) -> Result<CaptureRelayAcknowledgement, DeviceClientError> {
        let (lock, changed) = &*self.shared;
        let mut state = lock
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        let provider = provider_for_token(&state, &call.provider_token)?.clone();
        let job = state
            .jobs
            .get_mut(&call.request_id)
            .ok_or(DeviceClientError::TransportUnavailable)?;
        if job.provider_device_id != provider {
            return Err(DeviceClientError::AuthorizationDenied);
        }
        let descriptor = response_transfer(job, &call.chunk.transfer_id)?;
        validate_chunk_shape(&call.chunk, descriptor, job.request_hash())?;
        let key = (call.chunk.transfer_id.clone(), call.chunk.chunk_index);
        if let Some(existing) = job.chunks.get(&key) {
            if existing != &call.chunk {
                return Err(DeviceClientError::ProtocolViolation);
            }
        } else {
            job.chunks.insert(key, call.chunk);
            changed.notify_all();
        }
        Ok(CaptureRelayAcknowledgement {
            request_id: call.request_id,
            accepted: true,
        })
    }

    pub(crate) fn fetch_chunk(
        &self,
        call: &CaptureChunkFetchCall,
    ) -> Result<CaptureChunkFetchResult, DeviceClientError> {
        let (lock, _) = &*self.shared;
        let state = lock
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        let job = state
            .jobs
            .get(&call.request_id)
            .filter(|job| job.delivery_token == call.delivery_token)
            .ok_or(DeviceClientError::AuthorizationDenied)?;
        let descriptor = response_transfer(job, &call.transfer_id)?;
        if call.chunk_index >= descriptor.chunk_count {
            return Err(DeviceClientError::ProtocolViolation);
        }
        Ok(job
            .chunks
            .get(&(call.transfer_id.clone(), call.chunk_index))
            .cloned()
            .map_or(
                CaptureChunkFetchResult::Pending,
                CaptureChunkFetchResult::Available,
            ))
    }

    pub(crate) fn receipt(
        &self,
        request_id: &CaptureRequestId,
        requester: &DeviceId,
    ) -> Result<CaptureRequestRelayReceipt, DeviceClientError> {
        let (lock, _) = &*self.shared;
        let state = lock
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        let job = state
            .jobs
            .get(request_id)
            .filter(|job| job.request.requester_device_id == *requester)
            .ok_or(DeviceClientError::AuthorizationDenied)?;
        Ok(CaptureRequestRelayReceipt {
            response: job.response.clone().ok_or(DeviceClientError::NoProgress)?,
            delivery_token: job.delivery_token.clone(),
            wrapped_keys: job.wrapped_keys.clone(),
        })
    }

    pub(crate) fn acknowledge_delivery(
        &self,
        call: &CaptureDeliveryAcknowledgeCall,
    ) -> Result<CaptureRelayAcknowledgement, DeviceClientError> {
        let (lock, _) = &*self.shared;
        let mut state = lock
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        let job = state
            .jobs
            .get(&call.request_id)
            .filter(|job| job.delivery_token == call.delivery_token)
            .ok_or(DeviceClientError::AuthorizationDenied)?;
        if !all_chunks_present(job)? {
            return Err(DeviceClientError::NoProgress);
        }
        state.jobs.remove(&call.request_id);
        Ok(CaptureRelayAcknowledgement {
            request_id: call.request_id.clone(),
            accepted: true,
        })
    }

    fn enqueue_and_wait(
        &self,
        context: &RuntimeDeviceCooperationContext,
        request: poche_protocol::CaptureRequestWire,
        provider_device_id: &DeviceId,
    ) -> Result<CaptureResponseWire, DeviceClientError> {
        let (lock, changed) = &*self.shared;
        let mut state = lock
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        if !state.providers.contains_key(provider_device_id)
            || state.jobs.contains_key(&request.request_id)
            || context.requester_certificate.device_id != request.requester_device_id
            || context.provider_certificate.device_id != *provider_device_id
        {
            return Err(DeviceClientError::AuthorizationDenied);
        }
        let request_id = request.request_id.clone();
        state.jobs.insert(
            request_id.clone(),
            RelayJobState {
                provider_device_id: provider_device_id.clone(),
                requester_certificate: context.requester_certificate.clone(),
                request,
                delivery_token: CaptureRelayToken::random()
                    .map_err(|_| DeviceClientError::KeyUnavailable)?,
                response: None,
                wrapped_keys: Vec::new(),
                chunks: BTreeMap::new(),
            },
        );
        changed.notify_all();
        let (state, timeout) = changed
            .wait_timeout_while(state, PROVIDER_RESPONSE_TIMEOUT, |current| {
                current
                    .jobs
                    .get(&request_id)
                    .is_some_and(|job| job.response.is_none())
            })
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        if timeout.timed_out() {
            return Err(DeviceClientError::TransportUnavailable);
        }
        state
            .jobs
            .get(&request_id)
            .and_then(|job| job.response.clone())
            .ok_or(DeviceClientError::TransportUnavailable)
    }
}

impl RuntimeDeviceCooperationHandler for RelayCaptureHandler {
    fn cooperate(
        &mut self,
        context: &RuntimeDeviceCooperationContext,
        request: DeviceCooperationRequest,
    ) -> Result<DeviceCooperationResult, DeviceClientError> {
        let DeviceCooperationRequest::Capture(request) = request else {
            return Err(DeviceClientError::ProtocolViolation);
        };
        self.relay
            .enqueue_and_wait(context, request, &self.provider_device_id)
            .map(DeviceCooperationResult::Capture)
    }
}

impl RelayJobState {
    fn request_hash(&self) -> poche_protocol::SemanticHash {
        poche_protocol::capture_request_hash(&self.request.unsigned())
            .expect("an authorized capture request has canonical bytes")
    }
}

fn provider_for_token<'a>(
    state: &'a RelayState,
    token: &CaptureRelayToken,
) -> Result<&'a DeviceId, DeviceClientError> {
    token
        .validate()
        .map_err(|_| DeviceClientError::AuthorizationDenied)?;
    state
        .providers
        .iter()
        .find(|(_, provider)| provider.token == *token)
        .map(|(device, _)| device)
        .ok_or(DeviceClientError::AuthorizationDenied)
}

fn validate_response_keys(
    job: &RelayJobState,
    response: &CaptureResponseWire,
    wrapped_keys: &[WrappedCaptureTransferKey],
) -> Result<(), DeviceClientError> {
    if response.request_id != job.request.request_id
        || response.requester_device_id != job.request.requester_device_id
        || response.provider_device_id != job.provider_device_id
        || response.request_hash != job.request_hash()
    {
        return Err(DeviceClientError::ProtocolViolation);
    }
    let CaptureResponseOutcomeWire::Accepted { artifacts } = &response.outcome else {
        return if wrapped_keys.is_empty() {
            Ok(())
        } else {
            Err(DeviceClientError::ProtocolViolation)
        };
    };
    if wrapped_keys.len() != artifacts.len() {
        return Err(DeviceClientError::ProtocolViolation);
    }
    for (artifact, wrapped) in artifacts.iter().zip(wrapped_keys) {
        wrapped
            .validate()
            .map_err(|_| DeviceClientError::ProtocolViolation)?;
        if wrapped.recipient_device_id != job.request.requester_device_id
            || wrapped.recipient_certificate_id != job.requester_certificate.certificate_id
            || wrapped.transfer_id != artifact.transfer.transfer_id
            || wrapped.request_hash != response.request_hash
        {
            return Err(DeviceClientError::ProtocolViolation);
        }
    }
    Ok(())
}

fn response_transfer<'a>(
    job: &'a RelayJobState,
    transfer_id: &CaptureTransferId,
) -> Result<&'a CaptureTransferDescriptorWire, DeviceClientError> {
    let response = job.response.as_ref().ok_or(DeviceClientError::NoProgress)?;
    let CaptureResponseOutcomeWire::Accepted { artifacts } = &response.outcome else {
        return Err(DeviceClientError::ProtocolViolation);
    };
    artifacts
        .iter()
        .find(|artifact| artifact.transfer.transfer_id == *transfer_id)
        .map(|artifact| &artifact.transfer)
        .ok_or(DeviceClientError::ProtocolViolation)
}

fn validate_chunk_shape(
    chunk: &EncryptedCaptureChunk,
    descriptor: &CaptureTransferDescriptorWire,
    request_hash: poche_protocol::SemanticHash,
) -> Result<(), DeviceClientError> {
    if chunk.transfer_id != descriptor.transfer_id
        || chunk.request_hash != request_hash
        || chunk.chunk_count != descriptor.chunk_count
        || chunk.chunk_index >= descriptor.chunk_count
        || chunk.plaintext_length == 0
        || chunk.plaintext_length > descriptor.chunk_bytes
        || chunk.ciphertext.len()
            != usize::try_from(chunk.plaintext_length)
                .map_err(|_| DeviceClientError::ProtocolViolation)?
                .saturating_add(16)
    {
        Err(DeviceClientError::ProtocolViolation)
    } else {
        Ok(())
    }
}

fn all_chunks_present(job: &RelayJobState) -> Result<bool, DeviceClientError> {
    let response = job.response.as_ref().ok_or(DeviceClientError::NoProgress)?;
    let CaptureResponseOutcomeWire::Accepted { artifacts } = &response.outcome else {
        return Ok(true);
    };
    Ok(artifacts.iter().all(|artifact| {
        (0..artifact.transfer.chunk_count).all(|index| {
            job.chunks
                .contains_key(&(artifact.transfer.transfer_id.clone(), index))
        })
    }))
}
