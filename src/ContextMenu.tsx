import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";

export type MenuAction = {
  id: string;
  label: string;
  icon?: ReactNode;
  shortcut?: string;
  disabled?: boolean;
  danger?: boolean;
  separator?: boolean;
  run: () => void | Promise<unknown>;
};
export type MenuPosition = { x: number; y: number; anchor?: HTMLElement };
export function menuPosition(
  event: React.MouseEvent<HTMLElement> | React.KeyboardEvent<HTMLElement>,
): MenuPosition {
  const box = event.currentTarget.getBoundingClientRect();
  return {
    x: "clientX" in event && event.clientX ? event.clientX : box.left + 16,
    y: "clientY" in event && event.clientY ? event.clientY : box.top + 24,
    anchor: event.currentTarget,
  };
}

export default function ContextMenu({
  position,
  title,
  actions,
  onClose,
  onError,
}: {
  position: MenuPosition;
  title: string;
  actions: MenuAction[];
  onClose: () => void;
  onError: (error: unknown) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [point, setPoint] = useState({ x: position.x, y: position.y });
  const callbacks = useRef({ onClose, onError });
  callbacks.current = { onClose, onError };
  const chosen = useRef(false);
  useLayoutEffect(() => {
    chosen.current = false;
    const menu = ref.current!;
    const rect = menu.getBoundingClientRect();
    setPoint({
      x: Math.max(8, Math.min(position.x, window.innerWidth - rect.width - 8)),
      y: Math.max(
        8,
        Math.min(position.y, window.innerHeight - rect.height - 8),
      ),
    });
    menu.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  }, [position]);
  useEffect(() => {
    const outside = (e: Event) => {
      if (!ref.current?.contains(e.target as Node)) callbacks.current.onClose();
    };
    const close = () => callbacks.current.onClose();
    document.addEventListener("pointerdown", outside, true);
    document.addEventListener("scroll", outside, true);
    window.addEventListener("resize", close);
    window.addEventListener("blur", close);
    return () => {
      document.removeEventListener("pointerdown", outside, true);
      document.removeEventListener("scroll", outside, true);
      window.removeEventListener("resize", close);
      window.removeEventListener("blur", close);
    };
  }, []);
  return createPortal(
    <div
      ref={ref}
      className="context-menu"
      role="menu"
      aria-label={title}
      style={{ left: point.x, top: point.y }}
      onContextMenu={(e) => e.preventDefault()}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Escape" || e.key === "Tab") {
          e.preventDefault();
          onClose();
          position.anchor?.focus();
          return;
        }
        const items = [
          ...ref.current!.querySelectorAll<HTMLButtonElement>(
            "button:not(:disabled)",
          ),
        ];
        const index = items.indexOf(
          document.activeElement as HTMLButtonElement,
        );
        const next =
          e.key === "ArrowDown"
            ? (index + 1) % items.length
            : e.key === "ArrowUp"
              ? (index - 1 + items.length) % items.length
              : e.key === "Home"
                ? 0
                : e.key === "End"
                  ? items.length - 1
                  : -1;
        if (next >= 0) {
          e.preventDefault();
          items[next]?.focus();
        }
      }}
    >
      <div className="context-menu-title">{title}</div>
      {actions.map((action) => (
        <div key={action.id}>
          {action.separator && (
            <div role="separator" className="context-menu-separator" />
          )}
          <button
            type="button"
            role="menuitem"
            disabled={action.disabled}
            className={action.danger ? "danger" : ""}
            onClick={() => {
              if (chosen.current) return;
              chosen.current = true;
              onClose();
              position.anchor?.focus();
              try {
                Promise.resolve(action.run()).catch(callbacks.current.onError);
              } catch (error) {
                callbacks.current.onError(error);
              }
            }}
          >
            <span className="context-menu-icon">{action.icon}</span>
            <span>{action.label}</span>
            {action.shortcut && <kbd>{action.shortcut}</kbd>}
          </button>
        </div>
      ))}
    </div>,
    document.body,
  );
}
