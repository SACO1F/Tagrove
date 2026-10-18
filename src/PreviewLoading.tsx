import { FileText } from "lucide-react";
import { t as ui } from "./i18n";
import "./preview-loading.css";

export function PreviewLoading({ label }: { label: string }) {
  return (
    <div className="preview-loading" role="status" aria-live="polite">
      <div className="preview-loading-frame" aria-hidden="true">
        <FileText size={28} strokeWidth={1.4} />
        <div className="preview-loading-lines">
          <i />
          <i />
          <i />
          <i />
          <i />
        </div>
      </div>
      <span>{ui(label)}</span>
    </div>
  );
}
