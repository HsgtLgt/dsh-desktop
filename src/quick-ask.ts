import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";

const input = document.getElementById("qa-input") as HTMLInputElement;
const body = document.getElementById("qa-body") as HTMLElement;
const statusEl = document.getElementById("qa-status") as HTMLElement;
const outputEl = document.getElementById("qa-output") as HTMLElement;
const runningEl = document.getElementById("qa-running") as HTMLElement;
const runningText = document.getElementById("qa-running-text") as HTMLElement;
const copyBtn = document.getElementById("qa-copy") as HTMLButtonElement;

const appWindow = getCurrentWebviewWindow();

let busy = false;
let outputBuf: string[] = [];
let finalOutput = "";

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
  finalOutput = "";
  outputEl.textContent = "";
  copyBtn.classList.add("hidden");
  body.classList.remove("hidden");
  runningEl.classList.remove("hidden");
  runningText.textContent = "任务运行中…";
  input.disabled = true;
  void invoke("quick_ask", { task });
}

async function copyResult() {
  if (!finalOutput) return;
  try {
    await writeText(finalOutput);
    copyBtn.textContent = "✅ 已复制";
    setTimeout(() => {
      copyBtn.textContent = "复制结果";
    }, 1500);
  } catch {
    copyBtn.textContent = "复制失败";
  }
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
      finalOutput = outputBuf.join("");
      copyBtn.classList.remove("hidden");
      input.value = "";
      input.focus();
    } else if (kind === "error") {
      busy = false;
      runningEl.classList.add("hidden");
      input.disabled = false;
      statusEl.textContent = "❌ 出错";
      outputEl.textContent = text || "未知错误";
      finalOutput = text || "";
      copyBtn.classList.remove("hidden");
      input.value = "";
      input.focus();
    }
  });

  copyBtn.addEventListener("click", () => {
    void copyResult();
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

