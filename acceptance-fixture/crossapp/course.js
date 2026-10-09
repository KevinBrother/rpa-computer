"use strict";
(() => {
  const config = JSON.parse(document.getElementById("case-config").textContent);
  const events = [];
  const started = performance.now();
  let truncated = false, exported = false, dragging = false, dropped = false;
  let lastVertical = -1000, lastHorizontal = -1000, lastMotion = -1000;
  const result = document.getElementById("visible-result");
  const gate = document.getElementById("download-gate");
  const exportButton = document.getElementById("export-log");
  function log(kind, event, details = {}) {
    if (events.length >= 256) { truncated = true; result.textContent = "Event limit reached. Evidence incomplete; STOP. GUI verification: not performed."; return; }
    events.push({index:events.length + 1, elapsed_ms:Math.round(performance.now() - started), kind, trusted:event.isTrusted === true, ...details});
    result.textContent = `${events.length} local application events recorded. GUI verification: not performed.`;
  }
  window.addEventListener("scroll", event => {
    const now = performance.now();
    if (now - lastVertical > 120) { log("scroll_y", event, {position:Math.round(window.scrollY)}); lastVertical = now; }
  }, {passive:true});
  document.getElementById("horizontal-course").addEventListener("scroll", event => {
    const now = performance.now();
    if (now - lastHorizontal > 120) { log("scroll_x", event, {position:Math.round(event.currentTarget.scrollLeft)}); lastHorizontal = now; }
  }, {passive:true});
  document.getElementById("entry-form").addEventListener("submit", event => {
    event.preventDefault();
    const text = document.getElementById("entry-text").value;
    const choice = document.getElementById("entry-choice").value;
    log("form_submit", event, {text, choice});
    document.getElementById("form-result").textContent = text === config.expected_text && choice === config.expected_choice
      ? "Local form values match the visible instruction. Not independent GUI proof."
      : "Local form values differ. Preserve this first attempt; do not hide it with a retry.";
  });
  const source = document.getElementById("drag-source");
  const target = document.getElementById("drop-target");
  source.addEventListener("dragstart", event => {
    dragging = true; dropped = false;
    event.dataTransfer.setData("text/plain", config.nonce);
    event.dataTransfer.effectAllowed = "move";
    log("drag_start", event, {x:event.clientX, y:event.clientY});
  });
  target.addEventListener("dragover", event => {
    if (!dragging) return;
    event.preventDefault(); event.dataTransfer.dropEffect = "move";
    const now = performance.now();
    if (now - lastMotion > 120) { log("drag_motion", event, {x:event.clientX, y:event.clientY}); lastMotion = now; }
  });
  target.addEventListener("drop", event => {
    event.preventDefault();
    if (!dragging || event.dataTransfer.getData("text/plain") !== config.nonce) return;
    dropped = true;
    log("drop", event, {target:"drop-target", x:event.clientX, y:event.clientY});
    target.classList.add("received");
    target.textContent = "PARCEL RECEIVED — local application state only";
  });
  source.addEventListener("dragend", event => {
    log("drag_end", event, {dropped}); dragging = false;
  });
  gate.addEventListener("change", event => {
    log("download_gate", event, {confirmed:gate.checked});
    exportButton.disabled = !gate.checked || exported;
  });
  exportButton.addEventListener("click", event => {
    if (!gate.checked || exported) return;
    log("export_requested", event);
    const data = {schema:"windows-crossapp-browser-events-v1", case_id:config.case_id,
      run_id:config.run_id, trial_id:config.trial_id, nonce:config.nonce,
      gui_verified:false, untrusted_application_data:true, events_truncated:truncated,
      download_confirmation:gate.checked, events,
      final:{text:document.getElementById("entry-text").value, choice:document.getElementById("entry-choice").value, dropped}};
    const bytes = new TextEncoder().encode(JSON.stringify(data));
    if (bytes.length > 256 * 1024 || truncated) { result.textContent = "Export incomplete or oversized. STOP; do not claim success."; return; }
    const blob = new Blob([bytes], {type:"application/json"});
    const link = document.createElement("a");
    const url = URL.createObjectURL(blob);
    link.href = url; link.download = config.filename; link.click();
    exported = true; exportButton.disabled = true;
    result.textContent = "Download requested once. Location/bytes require supervisor review. GUI verification: not performed.";
    setTimeout(() => URL.revokeObjectURL(url), 2000);
  });
})();
