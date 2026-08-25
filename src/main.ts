import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";

interface BootEvent {
  stage: string;
  message: string;
  detail?: string | null;
}

const statusEl = document.getElementById("status") as HTMLElement;
const spinnerEl = document.getElementById("spinner") as HTMLElement;
const detailEl = document.getElementById("detail") as HTMLElement;
const wizardEl = document.getElementById("wizard") as HTMLElement;
const errorEl = document.getElementById("error") as HTMLElement;
const errorMsgEl = document.getElementById("error-msg") as HTMLElement;
const installBtn = document.getElementById("btn-install") as HTMLButtonElement;

function setStage(stage: string, message: string, detail?: string | null) {
  statusEl.textContent = message;

  const busy = ["detecting", "starting", "installing", "installed"].includes(
    stage,
  );
  spinnerEl.classList.toggle("hidden", !busy);

  const needRuntime = stage === "need-runtime" || stage === "need-node";
  wizardEl.classList.toggle("hidden", !needRuntime);
  const showError = stage === "error" || stage === "error-silent";
  errorEl.classList.toggle("hidden", !showError);
  detailEl.classList.toggle("hidden", !detail);

  if (detail) {
    detailEl.textContent = detail;
  }
  if (showError && detail) {
    errorMsgEl.textContent = detail;
  }
  if (needRuntime) {
    installBtn.disabled = false;
  }
}

async function syncBootStatus() {
  try {
    const status = await invoke<BootEvent | null>("get_boot_status");
    if (status?.stage) {
      setStage(status.stage, status.message, status.detail ?? null);
      return;
    }
  } catch (e) {
    console.debug("get_boot_status", e);
  }

  // No cached status yet (boot still racing) — wait briefly and retry once.
  await new Promise((r) => setTimeout(r, 300));
  try {
    const status = await invoke<BootEvent | null>("get_boot_status");
    if (status?.stage) {
      setStage(status.stage, status.message, status.detail ?? null);
    }
  } catch {
    /* ignore */
  }
}

window.addEventListener("DOMContentLoaded", () => {
  void listen<BootEvent>("boot-status", (e) => {
    const { stage, message, detail } = e.payload;
    setStage(stage, message, detail);
  });

  document.getElementById("btn-install")?.addEventListener("click", () => {
    installBtn.disabled = true;
    statusEl.textContent = "正在安装运行时…";
    spinnerEl.classList.remove("hidden");
    void invoke("install_runtime_cmd");
  });

  document.getElementById("btn-guide")?.addEventListener("click", () => {
    void openUrl("https://nodejs.org/zh-cn/download");
  });

  document.getElementById("btn-retry")?.addEventListener("click", () => {
    void invoke("start_boot");
  });

  document.getElementById("btn-quit")?.addEventListener("click", () => {
    void invoke("quit_app");
  });

  void syncBootStatus();
});
