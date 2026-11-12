import type { ObjectDirective } from "vue";

/** Browser adapter for a modal's focus, Escape, and background interaction. */
export const modalFocus: ObjectDirective<HTMLElement, () => void> = {
  mounted(element, binding) {
    const previous = document.activeElement;
    const blocked: { element: HTMLElement; inert: boolean }[] = [];
    let current: HTMLElement | null = element;
    while (current?.parentElement) {
      for (const sibling of current.parentElement.children) {
        if (sibling instanceof HTMLElement && sibling !== current) {
          blocked.push({ element: sibling, inert: sibling.inert });
          sibling.inert = true;
        }
      }
      current = current.parentElement;
      if (current === document.body) break;
    }
    const buttons = () =>
      [...element.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")].filter(
        (button) => !button.closest("[inert]"),
      );
    const keydown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        binding.value();
      }
      if (event.key !== "Tab") return;
      const targets = buttons();
      const index = targets.indexOf(document.activeElement as HTMLButtonElement);
      const next = event.shiftKey
        ? index <= 0
          ? targets.length - 1
          : index - 1
        : (index + 1) % targets.length;
      event.preventDefault();
      targets[next]?.focus();
    };
    element.addEventListener("keydown", keydown);
    buttons()[0]?.focus();
    cleanups.set(element, () => {
      element.removeEventListener("keydown", keydown);
      for (const item of blocked) item.element.inert = item.inert;
      if (previous instanceof HTMLElement && previous.isConnected) {
        previous.focus();
        return;
      }
      document.querySelector<HTMLElement>('h1[tabindex="-1"], h2[tabindex="-1"]')?.focus();
    });
  },
  unmounted(element) {
    cleanups.get(element)?.();
    cleanups.delete(element);
  },
};
const cleanups = new WeakMap<HTMLElement, () => void>();
