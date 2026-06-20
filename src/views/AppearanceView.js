import { escapeHtml } from "../utils/markdown.js";
import { card } from "./shared.js";

/**
 * AppearanceView — theme + platform style + motion + density (ref-plan §6.5).
 * Theme persistence is migrated from the old Model tab. Platform style, motion
 * and density are live in-memory overrides for v0.4.0 (DB persistence is the
 * plan's phase-2 item); they write to `data-platform` / `data-density` /
 * reduced-motion so the rest of the UI reacts immediately.
 */
export function renderAppearanceView(container, state, handlers) {
  container.innerHTML = `
    <div class="settings-page appearance-page">
      ${card(
        "主题",
        `
          <p class="card-hint">主题跟随系统，或手动选择浅色 / 深色。</p>
          <div class="segmented" role="tablist" aria-label="主题">
            ${seg("theme-mode", "system", "跟随系统", state.theme)}
            ${seg("theme-mode", "light", "浅色", state.theme)}
            ${seg("theme-mode", "dark", "深色", state.theme)}
          </div>
        `,
      )}

      ${card(
        "平台风格",
        `
          <p class="card-hint">覆盖按系统检测的平台外观（仅本次会话生效）。</p>
          <div class="segmented" role="tablist" aria-label="平台风格">
            ${seg("platform-style", "auto", "跟随系统", state.platformStyle)}
            ${seg("platform-style", "macos", "macOS", state.platformStyle)}
            ${seg("platform-style", "windows", "Windows", state.platformStyle)}
            ${seg("platform-style", "linux", "Linux", state.platformStyle)}
          </div>
        `,
      )}

      ${card(
        "动效",
        `
          <label class="checkbox-row">
            <input name="reduceMotion" type="checkbox" ${state.reduceMotion ? "checked" : ""} />
            <span>减少动画（覆盖系统 reduced-motion 偏好）</span>
          </label>
        `,
      )}

      ${card(
        "密度",
        `
          <p class="card-hint">紧凑密度收窄间距，适合小窗口。</p>
          <div class="segmented" role="tablist" aria-label="密度">
            ${seg("density", "comfortable", "舒适", state.density)}
            ${seg("density", "compact", "紧凑", state.density)}
          </div>
        `,
      )}
    </div>
  `;

  container.querySelectorAll("[data-theme-mode]").forEach((button) => {
    button.addEventListener("click", () => handlers.onSetTheme(button.dataset.themeMode));
  });
  container.querySelectorAll("[data-platform-style]").forEach((button) => {
    button.addEventListener("click", () => handlers.onSetPlatformStyle(button.dataset.platformStyle));
  });
  container.querySelectorAll("[data-density]").forEach((button) => {
    button.addEventListener("click", () => handlers.onSetDensity(button.dataset.density));
  });
  bindSegmentedKeyboard(container);
  const reduce = container.querySelector('[name="reduceMotion"]');
  if (reduce) {
    reduce.addEventListener("change", () => handlers.onSetReduceMotion(reduce.checked));
  }
}

function seg(group, value, label, current) {
  const active = (current || "system") === value;
  return `<button class="segmented-tab ${active ? "active" : ""}" type="button" data-${group}="${value}" role="tab" aria-selected="${active ? "true" : "false"}" tabindex="${active ? "0" : "-1"}">${escapeHtml(label)}</button>`;
}

function bindSegmentedKeyboard(container) {
  container.querySelectorAll('.segmented[role="tablist"]').forEach((tablist) => {
    tablist.addEventListener("keydown", (event) => {
      const keys = ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"];
      if (!keys.includes(event.key)) return;

      const tabs = Array.from(tablist.querySelectorAll('[role="tab"]'));
      if (!tabs.length) return;

      event.preventDefault();
      const current = Math.max(
        0,
        tabs.indexOf(document.activeElement),
        tabs.findIndex((tab) => tab.getAttribute("aria-selected") === "true"),
      );
      const delta = event.key === "ArrowLeft" || event.key === "ArrowUp" ? -1 : 1;
      let next = current;
      if (event.key === "Home") next = 0;
      else if (event.key === "End") next = tabs.length - 1;
      else next = (current + delta + tabs.length) % tabs.length;

      const selector = segmentedSelector(tabs[next]);
      tabs[next].click();
      const refocus = globalThis.requestAnimationFrame || ((callback) => setTimeout(callback, 0));
      refocus(() => (container.querySelector(selector) || tabs[next])?.focus());
    });
  });
}

function segmentedSelector(tab) {
  const entry = Object.entries(tab.dataset)[0];
  if (!entry) return '[role="tab"]';
  const [key, value] = entry;
  const attr = key.replace(/[A-Z]/g, (char) => `-${char.toLowerCase()}`);
  return `[data-${attr}="${value}"]`;
}
