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

  const busy = ["detecting", "starting", "installing", "installed"].includes(stage);
  spinnerEl.classList.toggle("hidden", !busy);

  wizardEl.classList.toggle("hidden", stage !== "need-node");
  errorEl.classList.toggle("hidden", stage !== "error");
  detailEl.classList.toggle("hidden", !detail);

  if (detail) {
    detailEl.textContent = detail;
  }
  if (stage === "error" && detail) {
    errorMsgEl.textContent = detail;
  }
  if (stage === "need-node") {
    installBtn.disabled = false;
  }
}

window.addEventListener("DOMContentLoaded", () => {
  void listen<BootEvent>("boot-status", (e) => {
    const { stage, message, detail } = e.payload;
    setStage(stage, message, detail);
  });

  document.getElementById("btn-install")?.addEventListener("click", () => {
    installBtn.disabled = true;
    statusEl.textContent = "正在准备便携版 Node.js…";
    spinnerEl.classList.remove("hidden");
    void invoke("install_node");
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

  // 启动引导流程
  void invoke("start_boot");
});
