import { tr } from "./i18n";
import { useState, type ReactNode } from "react";
import { X, Eye, EyeOff, LoaderCircle, ChevronDown } from "lucide-react";
export function Field({
  label,
  value,
  onChange,
  placeholder = "",
  type = "text",
  hint,
  required = false,
  readOnly = false,
}: {
  label: string;
  value: any;
  onChange: (v: any) => void;
  placeholder?: string;
  type?: string;
  hint?: string;
  required?: boolean;
  readOnly?: boolean;
}) {
  const [show, setShow] = useState(false);
  return (
    <label className="field">
      <span>
        {label}
        {required && <b className="required"> *</b>}
      </span>
      <div className="input-wrap">
        <input
          required={required}
          readOnly={readOnly}
          autoComplete="off"
          spellCheck={false}
          value={value ?? ""}
          placeholder={placeholder}
          type={type === "password" && show ? "text" : type}
          onChange={(e) =>
            onChange(
              type === "number"
                ? e.target.value === ""
                  ? null
                  : Number(e.target.value)
                : e.target.value,
            )
          }
        />
        {type === "password" && (
          <button
            type="button"
            className="icon-btn reveal"
            aria-label={show ? tr("Hide password") : tr("Show password")}
            onClick={() => setShow(!show)}
          >
            {show ? <EyeOff size={16} /> : <Eye size={16} />}
          </button>
        )}
      </div>
      {hint && <small>{hint}</small>}
    </label>
  );
}
export function Select({
  label,
  value,
  onChange,
  options,
  hint,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  options: { value: string; label: string }[];
  hint?: string;
}) {
  return (
    <label className="field">
      <span>{label}</span>
      <div className="select-wrap">
        <select value={value} onChange={(e) => onChange(e.target.value)}>
          {options.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
        <ChevronDown size={14} />
      </div>
      {hint && <small>{hint}</small>}
    </label>
  );
}
export function TextArea({
  label,
  value,
  onChange,
  placeholder = "",
  rows = 5,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  rows?: number;
}) {
  return (
    <label className="field">
      <span>{label}</span>
      <textarea
        spellCheck={false}
        rows={rows}
        value={value ?? ""}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)}
      />
    </label>
  );
}
export function Check({
  checked,
  onChange,
  children,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  children: ReactNode;
}) {
  return (
    <label className="check">
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span>{children}</span>
    </label>
  );
}
export function Modal({
  title,
  children,
  onClose,
  wide = false,
}: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  wide?: boolean;
}) {
  return (
    <div
      className="modal-shade"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <section
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className={"modal " + (wide ? "wide" : "")}
      >
        <header>
          <h2>{title}</h2>
          <button
            className="icon-btn"
            aria-label={tr("Close dialog")}
            onClick={onClose}
          >
            <X size={18} />
          </button>
        </header>
        {children}
      </section>
    </div>
  );
}
export function Busy({ label = tr("Working…") }: { label?: string }) {
  return (
    <span className="busy">
      <LoaderCircle size={16} className="spin" />
      {label}
    </span>
  );
}
export function Empty({
  icon,
  title,
  detail,
  children,
}: {
  icon: ReactNode;
  title: string;
  detail: string;
  children?: ReactNode;
}) {
  return (
    <div className="empty">
      <div className="empty-icon">{icon}</div>
      <h2>{title}</h2>
      <p>{detail}</p>
      {children}
    </div>
  );
}
export function Pill({ children }: { children: ReactNode }) {
  return <span className="pill">{children}</span>;
}
