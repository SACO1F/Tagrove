import { t as ui, tf } from "./i18n";
import { useEffect, useState } from "react";
import { Files, Tags, Search, PackageOpen, Sparkles } from "lucide-react";
import { Modal } from "./components";
import { AutoTagSettings } from "./AutoTagSettings";
import {
  readOnboarding,
  saveOnboarding,
  type OnboardingStep,
} from "./onboarding";
import "./first-run.css";

const instructions = [
  {
    icon: Files,
    title: "加入文件",
    text: "点击“加入文件”，或把文件、文件夹拖入工作台。原文件留在原来的位置。",
  },
  {
    icon: Tags,
    title: "添加标签",
    text: "选中文件添加标签，也可以打开标签浮窗，把标签拖到对应文件上。",
  },
  {
    icon: Search,
    title: "快速找回",
    text: "用搜索和左侧标签筛选找资料。Ctrl 多选，Space 预览。",
  },
  {
    icon: PackageOpen,
    title: "备份与交接",
    text: "资料包包含原文件和标注；标注备份不包含原文件。",
  },
];

export function FirstRunGuide({
  library,
  folderEnabled,
  aiConfigured,
  helpRequest,
  onNotice,
}: {
  library: string;
  folderEnabled: boolean;
  aiConfigured: boolean;
  helpRequest: number;
  onNotice: (text: string) => void;
}) {
  const [step, setStep] = useState<OnboardingStep>(() => {
    try {
      return readOnboarding(localStorage, library);
    } catch {
      return "ai";
    }
  });
  const [configure, setConfigure] = useState(false);
  const [saving, setSaving] = useState(false);
  useEffect(() => {
    if (helpRequest > 0) {
      setConfigure(false);
      setStep("guide");
    }
  }, [helpRequest]);
  const advance = (next: OnboardingStep) => {
    try {
      saveOnboarding(localStorage, library, next);
    } catch {
      onNotice(
        ui("引导状态未能保存，下次启动可能再次显示；文件和标签不受影响"),
      );
    }
    setConfigure(false);
    setStep(next);
  };
  if (step === "done") return null;
  if (step === "guide")
    return (
      <Modal
        key="guide"
        title={ui("快速上手拾签")}
        onClose={() => advance("done")}
      >
        <div className="modal-content first-run">
          <p className="muted">{ui("四步开始整理，让资料更容易被找到。")}</p>
          <ol className="first-run-steps">
            {instructions.map(({ icon: Icon, title, text }) => (
              <li key={title}>
                <span className="first-run-icon">
                  <Icon size={20} />
                </span>
                <div>
                  <strong>{ui(title)}</strong>
                  <p>{ui(text)}</p>
                </div>
              </li>
            ))}
          </ol>
          <p className="subtle">
            {ui("AI 可以稍后在偏好设置中配置；AI 建议标签需要你确认。")}
          </p>
          <div className="first-run-actions">
            <button className="button primary" onClick={() => advance("done")}>
              {ui("开始使用")}
            </button>
          </div>
        </div>
      </Modal>
    );
  return (
    <Modal
      key={configure ? "configure" : "ai"}
      title={configure ? ui("设置 AI 自动标注") : ui("启用 AI 自动标注？")}
      onClose={() => {
        if (!saving) advance("guide");
      }}
      dismissDisabled={saving}
    >
      <div className="modal-content first-run">
        {configure ? (
          <>
            <p className="muted">
              {ui(
                "填写服务信息并保存。需要自动识别时，请勾选启用 AI；也可以暂不设置。",
              )}
            </p>
            <AutoTagSettings
              folderEnabled={folderEnabled}
              onboarding
              onBusyChange={setSaving}
              onSaved={() => {
                setSaving(false);
                advance("guide");
              }}
            />
          </>
        ) : (
          <>
            <span className="first-run-hero">
              <Sparkles size={28} />
            </span>
            <p>
              {ui(
                "让 AI 分析新导入的图片和文档，优先匹配已有标签，并提出待确认的标签建议。",
              )}
            </p>
            <p className="muted">
              {ui(
                "启用后，识别内容会发送到你配置的 AI 服务，远程服务可能收费。暂不设置也能使用手工标注、搜索和标签浮窗。",
              )}
            </p>
            {aiConfigured && (
              <p className="subtle">
                {ui("已检测到 AI 配置，可以保留或调整。")}
              </p>
            )}
          </>
        )}
        <div className="first-run-actions">
          <button
            className="button"
            disabled={saving}
            onClick={() => advance("guide")}
          >
            {aiConfigured ? ui("保持当前设置") : ui("暂不设置")}
          </button>
          {!configure && (
            <button
              className="button primary"
              onClick={() => setConfigure(true)}
            >
              {ui("设置 AI")}
            </button>
          )}
        </div>
      </div>
    </Modal>
  );
}
