// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import init, { veilidClient } from "./pkg/veilid_wasm.js";

const elements = {
  attachment: document.querySelector("#attachment"),
  bootstrap: document.querySelector("#bootstrap"),
  environment: document.querySelector("#environment"),
  peers: document.querySelector("#peers"),
  ready: document.querySelector("#ready"),
  result: document.querySelector("#result"),
  shutdown: document.querySelector("#shutdown"),
  start: document.querySelector("#start"),
  updates: document.querySelector("#updates"),
  version: document.querySelector("#version"),
};

const query = new URLSearchParams(location.search);
elements.bootstrap.value = query.get("bootstrap") ??
  "ws://bootstrap-v1.veilid.net:5150/ws";
elements.environment.textContent =
  `Origin: ${location.origin}; secure context: ${globalThis.isSecureContext}`;

const updateLines = [];
let started = false;
let attached = false;

function describeError(error) {
  if (typeof error === "string") return error;
  try {
    return JSON.stringify(error);
  } catch (_) {
    return String(error);
  }
}

function appendUpdate(update) {
  let summary;
  if (update.kind === "Log") {
    summary = `${update.kind}: ${update.logLevel}: ${update.message}`;
  } else if (update.kind === "Network") {
    summary = `Network: started=${update.started}; peers=${update.peers.length}`;
  } else if (update.kind === "Attachment") {
    summary = `Attachment: state=${update.state}; peers=${update.livePeerCount}; ` +
      `public-ready=${update.publicInternetReady}`;
  } else {
    summary = update.kind;
  }
  updateLines.push(summary);
  elements.updates.textContent = updateLines.slice(-80).join("\n");
  if (update.kind === "Attachment") renderAttachment(update);
}

function renderAttachment(attachment) {
  elements.attachment.textContent = attachment.state;
  elements.peers.textContent = String(attachment.livePeerCount);
  elements.ready.textContent = String(attachment.publicInternetReady);
}

async function waitForNetwork(timeoutMs) {
  const deadline = performance.now() + timeoutMs;
  while (performance.now() < deadline) {
    const state = await veilidClient.getState();
    renderAttachment(state.attachment);
    if (state.attachment.publicInternetReady &&
        Number(state.attachment.livePeerCount) > 0) {
      return state;
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  return veilidClient.getState();
}

async function start() {
  elements.start.disabled = true;
  elements.result.textContent = "Loading pinned Veilid WASM…";
  const startTime = performance.now();
  try {
    await init();
    elements.version.textContent = veilidClient.versionString();
    await veilidClient.initializeCore({
      logging: {
        api: { enabled: true, level: "Info" },
        performance: {
          enabled: true,
          level: "Info",
          console: { enabled: true, color: false, timestamp: true },
        },
      },
    });

    const config = veilidClient.defaultConfig();
    config.programName = "poche-browser-transport-spike";
    config.namespace = "ephemeral-probe";
    config.tableStore.delete = true;
    config.protectedStore.delete = true;
    config.blockStore.delete = true;
    config.network.routingTable.bootstrap = [elements.bootstrap.value];
    config.network.protocol.ws.connect = true;
    config.network.protocol.ws.listen = false;
    if (config.network.protocol.wss) {
      config.network.protocol.wss.connect = true;
      config.network.protocol.wss.listen = false;
    }

    await veilidClient.startupCore(appendUpdate, config);
    started = true;
    elements.shutdown.disabled = false;
    await veilidClient.attach();
    attached = true;
    const state = await waitForNetwork(20_000);
    const elapsed = Math.round(performance.now() - startTime);
    const success = state.attachment.publicInternetReady &&
      Number(state.attachment.livePeerCount) > 0;
    elements.result.textContent = success
      ? `PASS: browser node reached the public network in ${elapsed} ms.`
      : `FAIL: no ready public network after ${elapsed} ms; ` +
        `state=${state.attachment.state}, peers=${state.attachment.livePeerCount}.`;
    globalThis.pocheVeilidProbe = { bootstrap: elements.bootstrap.value, elapsed, state, success };
  } catch (error) {
    const elapsed = Math.round(performance.now() - startTime);
    const message = describeError(error);
    elements.result.textContent = `ERROR after ${elapsed} ms: ${message}`;
    globalThis.pocheVeilidProbe = { bootstrap: elements.bootstrap.value, elapsed, error: message, success: false };
  } finally {
    elements.start.disabled = started;
  }
}

async function shutdown() {
  elements.shutdown.disabled = true;
  try {
    if (attached) {
      await veilidClient.detach();
      attached = false;
    }
    if (started) {
      await veilidClient.shutdownCore();
      started = false;
    }
    elements.result.textContent = "Browser node shut down.";
  } catch (error) {
    elements.result.textContent = `Shutdown error: ${describeError(error)}`;
  } finally {
    elements.start.disabled = started;
  }
}

elements.start.addEventListener("click", start);
elements.shutdown.addEventListener("click", shutdown);
elements.result.textContent = "Ready to load Veilid WASM.";
