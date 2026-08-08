/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

const session = document.body.dataset.gameSession;
const roomEvents = new EventSource(`/game/${encodeURIComponent(session)}/events`);

roomEvents.addEventListener("room", (event) => {
  const template = document.createElement("template");
  template.innerHTML = event.data;
  const next = template.content.firstElementChild;
  const current = document.getElementById("game-shell");
  if (!next || !current) return;

  const openDetails = [...current.querySelectorAll("details[open][id]")].map((detail) => detail.id);
  current.replaceWith(next);
  for (const id of openDetails) {
    const detail = document.getElementById(id);
    if (detail) detail.open = true;
  }
  if (next.dataset.sessionEnded === "true") roomEvents.close();
});

async function postForm(form, submitter) {
  const confirmation = submitter?.dataset.confirm;
  if (confirmation && !window.confirm(confirmation)) return;

  const chat = form.matches("[data-chat-form]");
  const options = { method: "POST" };
  if (chat) {
    options.headers = { "Content-Type": "application/x-www-form-urlencoded;charset=UTF-8" };
    options.body = new URLSearchParams(new FormData(form)).toString();
  }
  const response = await fetch(form.action, options);
  if (chat) {
    const status = form.querySelector("[data-chat-status]");
    if (response.ok) {
      form.reset();
      if (status) status.textContent = "Message sent.";
    } else if (status) {
      status.textContent = await response.text();
    }
  }
}

document.addEventListener("submit", (event) => {
  const form = event.target.closest("#game-shell form");
  if (!form) return;
  event.preventDefault();
  void postForm(form, event.submitter);
});

async function writeClipboard(text) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch (_) {
    const scratch = document.createElement("textarea");
    scratch.value = text;
    scratch.style.position = "fixed";
    scratch.style.opacity = "0";
    document.body.appendChild(scratch);
    scratch.select();
    const copied = document.execCommand("copy");
    scratch.remove();
    return copied;
  }
}

document.addEventListener("click", (event) => {
  const opener = event.target.closest("[data-open-details]");
  if (opener) {
    const detail = document.getElementById(opener.dataset.openDetails);
    if (detail) {
      detail.open = true;
      detail.scrollIntoView({ behavior: "smooth", block: "nearest" });
      detail.querySelector("input, textarea, button")?.focus();
    }
    return;
  }

  const diagnostic = event.target.closest("[data-copy-context]");
  if (diagnostic) {
    const target = document.getElementById(diagnostic.dataset.copyContext);
    const status = document.getElementById(diagnostic.dataset.copyStatus);
    if (!target) return;
    void writeClipboard(target.value).then((copied) => {
      if (status) status.textContent = copied
        ? "Copied safe diagnostic context to the clipboard."
        : "Clipboard access was unavailable; the text remains selectable.";
    });
    return;
  }

  const copy = event.target.closest("[data-copy-text]");
  if (!copy) return;
  const target = document.getElementById(copy.dataset.copyText);
  if (!target) return;
  const previous = copy.textContent;
  void writeClipboard(target.textContent).then((copied) => {
    copy.textContent = copied ? "Copied" : "Select code";
    setTimeout(() => { copy.textContent = previous; }, 1400);
  });
});

let dragged = null;
document.addEventListener("dragstart", (event) => {
  dragged = event.target.closest("[data-command-id]")?.dataset.commandId ?? null;
});
document.addEventListener("drop", (event) => {
  const target = event.target.closest("#play-target");
  if (!target || !dragged) return;
  event.preventDefault();
  void fetch(`/game/${encodeURIComponent(session)}/command/${encodeURIComponent(dragged)}`, {
    method: "POST",
  });
});
document.addEventListener("dragover", (event) => {
  if (event.target.closest("#play-target")) event.preventDefault();
});
