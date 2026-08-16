import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

const input = document.getElementById("qa-input") as HTMLInputElement;
const body = document.getElementById("qa-body") as HTMLElement;
const statusEl = document.getElementById("qa-status") as HTMLElement;
const outputEl = document.getElementById("qa-output") as HTMLElement;
const runningEl = document.getElementById("qa-running") as HTMLElement;
const runningText = document.getElementById("qa-running-text") as HTMLElement;

const appWindow = getCurrentWebviewWindow();

let busy = false;
let outputBuf: string[] = [];

interface QuickAskEvent {
  kind: "started" | "output" | "done" | "error";
  text: string;
  exitCode?: number | null;
}

function submit() {
  if (busy) return;
  const task = input.value.trim();
  if (!task) return;

  busy = true;
  outputBuf = [];
  outputEl.textContent = "";
  body.classList.remove("hidden");
  runningEl.classList.remove("hidden");
  runningText.textContent = "任务运行中…";
  input.disabled = true;
  void invoke("quick_ask", { task });
}

window.addEventListener("DOMContentLoaded", () => {
  void listen<QuickAskEvent>("quick-ask-event", (e) => {
    const { kind, text } = e.payload;
    if (kind === "output") {
      outputBuf.push(text);
      outputEl.textContent = outputBuf.join("");
      outputEl.scrollTop = outputEl.scrollHeight;
    } else if (kind === "done") {
      busy = false;
      runningEl.classList.add("hidden");
      input.disabled = false;
      statusEl.textContent = "✅ 完成";
      input.value = "";
      input.focus();
    } else if (kind === "error") {
      busy = false;
      runningEl.classList.add("hidden");
      input.disabled = false;
      statusEl.textContent = "❌ 出错";
      outputEl.textContent = text || "未知错误";
      input.value = "";
      input.focus();
    }
  });

  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      submit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      void invoke("hide_quick_ask");
    }
  });

  // focus the input whenever the popup becomes visible
  appWindow.onFocusChanged(({ payload: focused }) => {
    if (focused && !busy) {
      input.focus();
      input.select();
    }
  });

  void invoke("quick_ask_ready");
});
