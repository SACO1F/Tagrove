import { useId, useState, type ReactNode } from "react";
import { ChevronRight } from "lucide-react";
import "./settings-sections.css";

/** Keep form state mounted while making collapsed controls inaccessible. */
export function SettingsSection({
  title,
  children,
  className = "",
  collapsible = true,
}: {
  title: ReactNode;
  children: ReactNode;
  className?: string;
  collapsible?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const content = useId();
  if (!collapsible)
    return (
      <section className={className}>
        <h3>{title}</h3>
        {children}
      </section>
    );
  return (
    <section className={`settings-section ${className}`} data-open={open}>
      <h3 className="settings-section-heading">
        <button
          type="button"
          aria-expanded={open}
          aria-controls={content}
          onClick={() => setOpen((value) => !value)}
        >
          <span>{title}</span>
          <ChevronRight size={17} aria-hidden="true" />
        </button>
      </h3>
      <div
        className="settings-section-reveal"
        id={content}
        inert={!open}
        aria-hidden={!open}
      >
        <div className="settings-section-clip">
          <div className="settings-section-body">{children}</div>
        </div>
      </div>
    </section>
  );
}
