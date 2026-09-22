import { browser } from "@wdio/globals";

// The embedded driver omits dblclick and the select change event. Dispatch
// those complete DOM events in the native WebView. Playwright separately
// verifies the real browser mouse/keyboard sequences.
export async function doubleClick(element) {
  const target = await element;
  await target.waitForDisplayed();
  await browser.execute(
    (el) =>
      el.dispatchEvent(
        new MouseEvent("dblclick", {
          bubbles: true,
          cancelable: true,
          button: 0,
          detail: 2,
        }),
      ),
    target,
  );
}
export async function selectValue(element, value) {
  const target = await element;
  await target.waitForDisplayed();
  await browser.execute(
    (el, value) => {
      el.value = value;
      el.dispatchEvent(new Event("change", { bubbles: true }));
    },
    target,
    value,
  );
}
