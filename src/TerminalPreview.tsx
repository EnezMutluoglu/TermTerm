import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { terminalTheme, terminalFontFamily } from "./terminalThemes";
import "@xterm/xterm/css/xterm.css";

export default function TerminalPreview({ themeId, fontSize }: { themeId: string; fontSize: number }) {
  const container = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const terminal = new Terminal({ theme: terminalTheme(themeId), fontSize, fontFamily: terminalFontFamily, lineHeight: 1.25, minimumContrastRatio: 4.5, disableStdin: true, scrollback: 0, cursorBlink: false, rows: 6 });
    const fit = new FitAddon(); terminal.loadAddon(fit); terminal.open(container.current!);
    const observer = new ResizeObserver(() => { if (container.current?.clientWidth) fit.fit(); });
    observer.observe(container.current!);
    terminal.write('\x1b[?25l\x1b[1;32myou@server\x1b[0m:\x1b[1;34m~/projects\x1b[0m $ git status\r\n\x1b[32m✓ Connected\x1b[0m  \x1b[33m● Warning\x1b[0m  \x1b[31m✕ Error\x1b[0m\r\n');
    for (let row = 0; row < 2; row++) {
      for (let color = 0; color < 8; color++) terminal.write(`\x1b[${(row ? 90 : 30) + color}m ANSI ${color + row * 8} \x1b[0m`);
      terminal.write('\r\n');
    }
    terminal.write('Türkçe: ğüşöçıİ  •  0123456789  •  ┌─┬─┐');
    return () => { observer.disconnect(); terminal.dispose(); };
  }, [themeId, fontSize]);
  return <div className="terminal-sample" aria-label="Live terminal color preview" ref={container} style={{ background: terminalTheme(themeId).background }} />;
}
