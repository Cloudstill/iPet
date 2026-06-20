import { icon } from "../ui/icons.js";
import { escapeHtml } from "../utils/markdown.js";
import { card, escapeAttr } from "./shared.js";

const DEFAULT_PERSONA = {
  displayName: "iPet",
  role: "常驻桌面的轻量助手，帮助用户处理日常问答、系统状态检查和本地任务。",
  tone: "清晰、直接、友好，不刻意卖萌。",
  responseLength: "默认简洁，复杂问题先给结论，再给必要步骤。",
  proactivity: "在信息不足时先做合理假设；风险较高或会改变本机状态时先确认。",
  toolPolicy: "需要系统状态、文件或目录信息时，可以主动建议或使用可用工具；执行本地命令前说明目的。",
  focusAreas: "编程、系统排查、文件整理、效率工具、中文沟通。",
  boundaries: "不编造事实；不隐藏失败；不替用户做高风险不可逆操作。",
  extraInstructions: "",
};

const PRESETS = [
  {
    id: "balanced",
    title: "稳重助手",
    desc: "适合日常使用，清晰、克制、重执行。",
    values: DEFAULT_PERSONA,
  },
  {
    id: "technical",
    title: "工程搭档",
    desc: "更关注代码、排查和可验证结论。",
    values: {
      ...DEFAULT_PERSONA,
      role: "常驻桌面的工程搭档，优先帮助用户阅读代码、定位问题、实现改动并验证结果。",
      tone: "专业、直接、少寒暄。",
      responseLength: "先给结论和改动摘要，再列验证结果；避免长篇解释。",
      focusAreas: "代码实现、测试、构建、系统排查、自动化脚本。",
    },
  },
  {
    id: "companion",
    title: "轻陪伴",
    desc: "语气更柔和，但仍保持简洁可执行。",
    values: {
      ...DEFAULT_PERSONA,
      role: "常驻桌面的轻陪伴助手，帮助用户保持节奏、整理想法并完成具体任务。",
      tone: "温和、耐心、自然，但不夸张。",
      responseLength: "短问题短答；任务型问题给清晰步骤。",
      focusAreas: "日程整理、想法记录、文件归纳、日常问答、系统状态提醒。",
    },
  },
];

const FIELDS = [
  "displayName",
  "role",
  "tone",
  "responseLength",
  "proactivity",
  "toolPolicy",
  "focusAreas",
  "boundaries",
  "extraInstructions",
];

const LINE_LABELS = {
  displayName: "名称",
  role: "身份定位",
  tone: "沟通语气",
  responseLength: "回答长度",
  proactivity: "主动程度",
  toolPolicy: "工具策略",
  focusAreas: "关注重点",
  boundaries: "边界与禁忌",
  extraInstructions: "补充要求",
};

export function renderPersonaView(container, state, handlers) {
  const draft = state.settingsDraft ?? {};
  const prompt = draft.systemPrompt || "";
  const persona = parsePersonaPrompt(prompt);

  container.innerHTML = `
    <form class="settings-form settings-page persona-form" data-role="persona-form">
      ${state.personaOnboardingVisible ? renderOnboarding() : ""}

      ${card(
        "人设预设",
        `
          <p class="card-hint">选择一个起点后仍可继续微调每一项。</p>
          <div class="persona-preset-grid">
            ${PRESETS.map((preset) => renderPresetButton(preset)).join("")}
          </div>
        `,
      )}

      <div class="persona-layout">
        <div class="persona-main">
          ${card(
            "基础身份",
            `
              <div class="settings-grid">
                <label>
                  <span>名称</span>
                  <input name="displayName" data-persona-field value="${escapeAttr(persona.displayName)}" />
                </label>
                <label>
                  <span>沟通语气</span>
                  <input name="tone" data-persona-field value="${escapeAttr(persona.tone)}" />
                </label>
              </div>
              <label>
                <span>身份定位</span>
                <textarea name="role" data-persona-field rows="3">${escapeHtml(persona.role)}</textarea>
              </label>
            `,
          )}

          ${card(
            "行为方式",
            `
              <label>
                <span>回答长度与结构</span>
                <textarea name="responseLength" data-persona-field rows="3">${escapeHtml(persona.responseLength)}</textarea>
              </label>
              <label>
                <span>主动程度</span>
                <textarea name="proactivity" data-persona-field rows="3">${escapeHtml(persona.proactivity)}</textarea>
              </label>
              <label>
                <span>工具策略</span>
                <textarea name="toolPolicy" data-persona-field rows="3">${escapeHtml(persona.toolPolicy)}</textarea>
              </label>
            `,
          )}

          ${card(
            "偏好与边界",
            `
              <label>
                <span>关注重点</span>
                <textarea name="focusAreas" data-persona-field rows="3">${escapeHtml(persona.focusAreas)}</textarea>
              </label>
              <label>
                <span>边界与禁忌</span>
                <textarea name="boundaries" data-persona-field rows="3">${escapeHtml(persona.boundaries)}</textarea>
              </label>
              <label>
                <span>补充要求</span>
                <textarea name="extraInstructions" data-persona-field rows="3" placeholder="例如：默认使用中文；代码回答必须附验证命令。">${escapeHtml(persona.extraInstructions)}</textarea>
              </label>
            `,
          )}
        </div>
        <aside class="persona-side">
          <section class="settings-card persona-preview">
            <div class="persona-preview-head">
              <h3>最终 System Prompt</h3>
              <button class="icon-button" type="button" data-action="rebuild-persona-prompt" title="重新生成" aria-label="重新生成">${icon("refresh")}</button>
            </div>
            <p class="card-hint">这里是实际保存给模型的完整人设提示词。需要精修时可以直接编辑。</p>
            <textarea name="systemPrompt" rows="10">${escapeHtml(prompt || buildPersonaPrompt(persona))}</textarea>
          </section>
        </aside>
      </div>

      <div class="form-actions">
        ${state.personaOnboardingVisible ? `<button class="text-button" type="button" data-action="dismiss-persona-guide">稍后再说</button>` : ""}
        <button class="text-button primary" type="submit">${icon("check")} 保存人设</button>
      </div>
    </form>
  `;

  bindPersonaForm(container, handlers);
}

function renderOnboarding() {
  return `
    <section class="settings-status persona-onboarding">
      <strong>先设定 iPet 的人设</strong>
      <span>这会影响 iPet 的身份、语气、主动性和工具使用边界。完成后可以随时回来修改。</span>
    </section>
  `;
}

function renderPresetButton(preset) {
  return `
    <button class="persona-preset" type="button" data-persona-preset="${preset.id}">
      <strong>${escapeHtml(preset.title)}</strong>
      <span>${escapeHtml(preset.desc)}</span>
    </button>
  `;
}

function bindPersonaForm(container, handlers) {
  const form = container.querySelector('[data-role="persona-form"]');
  if (!form) return;

  const prompt = form.elements.systemPrompt;
  form.querySelectorAll("[data-persona-field]").forEach((field) => {
    field.addEventListener("input", () => {
      prompt.value = buildPersonaPrompt(readPersonaForm(form));
    });
  });

  form.querySelector('[data-action="rebuild-persona-prompt"]')?.addEventListener("click", () => {
    prompt.value = buildPersonaPrompt(readPersonaForm(form));
  });

  form.querySelectorAll("[data-persona-preset]").forEach((button) => {
    button.addEventListener("click", () => {
      const preset = PRESETS.find((item) => item.id === button.dataset.personaPreset);
      if (!preset) return;
      writePersonaForm(form, preset.values);
      prompt.value = buildPersonaPrompt(readPersonaForm(form));
    });
  });

  form.querySelector('[data-action="dismiss-persona-guide"]')?.addEventListener("click", () => {
    handlers.onDismissPersonaGuide?.();
  });

  form.addEventListener("submit", (event) => {
    event.preventDefault();
    const value = prompt.value.trim() || buildPersonaPrompt(readPersonaForm(form));
    handlers.onSavePersona?.({ systemPrompt: value });
  });
}

function readPersonaForm(form) {
  return FIELDS.reduce((result, name) => {
    result[name] = form.elements[name]?.value.trim() ?? "";
    return result;
  }, {});
}

function writePersonaForm(form, values) {
  FIELDS.forEach((name) => {
    if (form.elements[name]) form.elements[name].value = values[name] || "";
  });
}

function buildPersonaPrompt(values) {
  const persona = { ...DEFAULT_PERSONA, ...values };
  return `# iPet 人设
名称：${persona.displayName}
身份定位：${persona.role}
沟通语气：${persona.tone}
回答长度：${persona.responseLength}
主动程度：${persona.proactivity}
工具策略：${persona.toolPolicy}
关注重点：${persona.focusAreas}
边界与禁忌：${persona.boundaries}
补充要求：${persona.extraInstructions || "无"}`;
}

function parsePersonaPrompt(prompt) {
  const persona = { ...DEFAULT_PERSONA };
  const text = String(prompt || "");
  for (const [field, label] of Object.entries(LINE_LABELS)) {
    const match = text.match(new RegExp(`^${escapeRegExp(label)}：(.+)$`, "m"));
    if (match) persona[field] = match[1].trim();
  }
  return persona;
}

function escapeRegExp(value) {
  return String(value).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
