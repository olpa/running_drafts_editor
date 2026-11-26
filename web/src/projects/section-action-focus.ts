import { nextTick } from "vue";

/** Restore keyboard focus within a Section when its action disappears. */
export async function withSectionActionFocus(
  event: MouseEvent,
  action: () => Promise<void>,
  fallback: () => HTMLElement | null,
): Promise<void> {
  const button = event.currentTarget;
  if (!(button instanceof HTMLButtonElement)) return;
  const document = button.ownerDocument;
  const hadFocus = document.activeElement === button;
  await action();
  await nextTick();
  if (hadFocus && !button.isConnected && document.activeElement === document.body) {
    fallback()?.focus();
  }
}
