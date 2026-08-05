// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use poche_protocol::{
    CodecError, EnvelopeValidationError, MAX_FRAME_BYTES, decode_frame_line, encode_frame_line,
};

const FIXTURE: &[u8] = include_bytes!("../../../fixtures/protocol/command-chat-v1.ndjson");

#[test]
fn malformed_control_corpus_fails_closed() {
    let body = &FIXTURE[..FIXTURE.len() - 1];

    assert_eq!(
        decode_frame_line(body),
        Err(CodecError::MissingLineTerminator)
    );

    let mut crlf = body.to_vec();
    crlf.extend_from_slice(b"\r\n");
    assert_eq!(
        decode_frame_line(&crlf),
        Err(CodecError::AmbiguousControlInput)
    );

    let mut doubled = FIXTURE.to_vec();
    doubled.extend_from_slice(FIXTURE);
    assert_eq!(
        decode_frame_line(&doubled),
        Err(CodecError::AmbiguousControlInput)
    );

    let mut leading_space = Vec::with_capacity(FIXTURE.len() + 1);
    leading_space.push(b' ');
    leading_space.extend_from_slice(FIXTURE);
    assert_eq!(
        decode_frame_line(&leading_space),
        Err(CodecError::NonCanonical)
    );

    assert_eq!(
        decode_frame_line(b"{\"frame\":\"unknown\",\"envelope\":{}}\n"),
        Err(CodecError::InvalidJson)
    );
    assert_eq!(decode_frame_line(b"{]\n"), Err(CodecError::InvalidJson));
    assert_eq!(
        decode_frame_line(&[0xff, b'\n']),
        Err(CodecError::InvalidUtf8)
    );
    assert_eq!(
        decode_frame_line(&vec![b'x'; MAX_FRAME_BYTES + 1]),
        Err(CodecError::Oversize)
    );

    let unknown_version = String::from_utf8(FIXTURE.to_vec())
        .unwrap()
        .replace("\"protocol_version\":1", "\"protocol_version\":2");
    assert_eq!(
        decode_frame_line(unknown_version.as_bytes()),
        Err(CodecError::InvalidEnvelope(
            EnvelopeValidationError::UnknownVersion
        ))
    );

    let mut unknown_field = String::from_utf8(body.to_vec()).unwrap();
    unknown_field.pop();
    unknown_field.push_str(",\"extra\":0}\n");
    assert_eq!(
        decode_frame_line(unknown_field.as_bytes()),
        Err(CodecError::InvalidJson)
    );
}

#[test]
fn deterministic_decoder_fuzz_corpus_never_panics_or_accepts_noncanonical_input() {
    let mut corpus = vec![FIXTURE.to_vec()];

    for index in 0..FIXTURE.len() - 1 {
        for mask in [0x01, 0x20, 0x80] {
            let mut mutation = FIXTURE.to_vec();
            mutation[index] ^= mask;
            corpus.push(mutation);
        }
    }

    let mut random = 0x6a09_e667_f3bc_c909_u64;
    for case in 0..10_000_usize {
        random = xorshift64(random);
        let length = usize::try_from(random % 1_025).unwrap();
        let mut bytes = Vec::with_capacity(length + 1);
        for _ in 0..length {
            random = xorshift64(random);
            bytes.push(random.to_le_bytes()[0]);
        }
        if case % 3 == 0 {
            bytes.push(b'\n');
        }
        corpus.push(bytes);
    }

    let mut accepted = 0_usize;
    for bytes in corpus {
        let outcome = std::panic::catch_unwind(|| decode_frame_line(&bytes));
        let decoded = outcome.expect("bounded decoder must never panic on arbitrary bytes");
        if let Ok(frame) = decoded {
            accepted += 1;
            assert_eq!(
                encode_frame_line(&frame).unwrap(),
                bytes,
                "every accepted frame must already be the unique canonical encoding"
            );
        }
    }
    assert!(accepted >= 1, "the valid seed fixture must remain accepted");
}

fn xorshift64(mut value: u64) -> u64 {
    value ^= value << 13;
    value ^= value >> 7;
    value ^= value << 17;
    value
}
