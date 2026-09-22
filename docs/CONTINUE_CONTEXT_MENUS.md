# Resume checkpoint — context menus

Resumed at the user's request and completed on 2026-09-22. Current results: **17 unit, 27 browser UI and 4 native Windows tests passed**; production frontend build passed. See [final validation report](VALIDATION_CONTEXT_MENUS_2026-09-22.md). No new installer/release was published. The remaining text below is the historical pause checkpoint, not a current outstanding-work list.

## Request

Single-click cards/rows to show read-only details. Remove selection checkboxes and the selection action toolbar. Put edit/delete/move/copy/duplicate and related actions in right-click menus, as in the documented Termius workflows. Preserve the user's folder-only host browsing and the recent softened anthracite/blue palette.

## Workspace and safety

- Current branch: `feature/terminal-usability`. Many earlier changes are uncommitted; preserve all of them.
- No stable release, tag, main merge or update-feed publication is authorized for this UI work.
- Production portable application and personal vaults must remain untouched.
- Dependencies are available in `node_modules`; do not clean/reinstall them unnecessarily.
- No test/build launched in this turn is still running at this checkpoint. The new native test has been written but has NOT been executed.
- Existing packages under `artifacts/release-0.3.3-dev.1` predate the current palette/menu edits; do not describe them as containing these changes.

## Implemented in source

- `src/ContextMenu.tsx`: common portal menu, viewport clamping, keyboard navigation, Escape/outside dismissal, disabled actions, async error handling and duplicate-click guard.
- `src/recordActions.ts`: recursive group duplication with internal ID remapping; external credentials/jump references preserved; group-safe moving; containment and inherited connection details. Rejects missing source/destination records.
- `src/RecordDetails.tsx`: read-only information panel; private keys/password values are not displayed.
- `src/context-menu.css`: soft palette-based menu and details styling.
- `src/App.tsx`: click shows details, double-click opens group/connects host, Ctrl/Shift selection without checkboxes. Context actions for hosts/groups/identities/snippets/tunnels/workspaces/logs/known hosts; terminal tab menu. Copy/cut/paste between groups, recursive duplicate/delete, move/copy dialog, selected encrypted backup. Group navigation also switches back to Hosts. Virtual Ungrouped stays separate from multi-selection.
- `src/Editor.tsx`: delete moved out of editor into context menu.
- `src/DataTools.tsx`: selected backup IDs passed to the backend; required dependency inclusion remains backend-controlled. Fixed the initially missing prop on the main App DataTools instance.
- `src/Sftp.tsx`: host menu can open that host in the right panel; file context actions for transfer, path copy, rename, remote external editor/permissions, single/multiple delete; background menu for mkdir/select all/refresh. Selected-file footer shows details. Multiple deletion reports failures and retains failed items for retry. This latest multi-delete addition has compiled but is not yet covered by a dedicated test.
- Removed per-card Connect/Run/checkbox/ellipsis controls and the selection toolbar. Terminal right-click paste remains unchanged; terminal tab actions use the tab header.

## Tests / evidence so far

- `pnpm test:unit`: **17 passed** (13 previous + 4 new record action tests). Ran before the final missing-record validation addition.
- Initial full UI run: **24 passed / 2 failed**. Failures were a test selector expecting `Label` instead of `Label *`, and the missing selected-backup prop. Both fixed.
- Targeted rerun `pnpm exec playwright test tests/ui/context-menu.spec.ts`: **6/6 passed**, including backup scope and all record-kind actions. This run predates the final small changes: connected snippet fallback, virtual selection guard, group Open navigation fix, missing-record validation, SFTP multi-delete.
- Latest `pnpm build`: **passed**, after all of those small code changes.
- Screenshots: `artifacts/context-review/host-menu.png` (visually inspected, no clipping), `sftp-menu.png` (still needs visual inspection).
- Existing interface/palette UI tests updated for group double-click and right-click Edit.
- Existing desktop group-navigation tests updated. `tests/desktop/vendor-import.mjs` now also double-clicks groups.
- New `tests/desktop/context-menu.mjs`: **written, NOT RUN**. Uses a disposable encrypted vault and temporary local files; tests recursive copy/remapping, move/delete, lock/reopen persistence, protected identity deletion, SFTP rename/copy/mkdir/delete. Preserves/restores the shared recent-vault pointer if it still refers to this test. Embedded WDIO contextmenu is dispatched in the real WebView; browser tests separately use trusted mouse/keyboard input.

## Resume next steps

1. Review the final source changes, especially SFTP multi-delete failure/retry and stale directory handling. Add a focused UI test for multiple deletion with one failed item. Check read-only details and menu targets after changes.
2. Run formatting, unit tests and the full UI suite once; build again only if source changes require it. Existing CSS has unused `.selection-bar`, `.card-menu`, `.card-connect` rules that can be removed carefully. No need to retest every unchanged backend subsystem.
3. Run the new native test with a dev server on localhost:1420 and the existing E2E binary. In PowerShell:
   - `pnpm.cmd dev` in the source workspace, separate session.
   - `$env:TERMTERM_BINARY = Join-Path $env:LOCALAPPDATA 'TermTerm-build/folder-fix-20260922/src-tauri/target/debug/termterm.exe'`
   - `$env:TERMTERM_SPEC = './tests/desktop/context-menu.mjs'`
   - `pnpm.cmd test:native`
   - This is a previously built debug/E2E Rust binary loading current Vite UI; Rust backend is unchanged by the menu work. Check availability first. Do not stop the user's production application.
   - The new test has not been debugged yet. Likely selectors to verify: `.file-list`, `.modal input`, `select` for the move dialog, exact group matching, WDIO read-only detail assertions. Do not weaken assertions to hide behavior defects.
   - `recent.json` is shared app state: the test saves original bytes to its `.lab/context-menu-*/recent-before.bin`, restores only if the pointer still names its test vault. Keep the cleanup even if a test fails.
4. Inspect native screenshots and finish a concise validation report/user guide update. Report mocked browser checks separately from native real-vault/filesystem checks; macOS/Linux have not been newly validated for this UI change.
5. Show the user the resulting context-menu/details view. No stable release publication. Rebuilding old installers is not necessary unless requested as the next step.

## Reference sources already consulted

- https://docs.termius.com/organize-and-connect-to-hosts/groups-and-tags — right-click Edit and group Quick Connect.
- https://docs.termius.com/organize-and-connect-to-hosts/managing-files-with-sftp — Copy to target directory and external editor context actions.
- These references support specific workflows, not a claim of exhaustive parity with every undocumented Termius menu.

Temporary `.tools/context-menu-edit.cjs`, `.tools/sftp-context-edit.cjs` and `.tools/update-context-tests.cjs` have already been applied. Do not rerun the transformation scripts.
