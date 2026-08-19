// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

const textInputTypes = new Set([
  "email",
  "number",
  "password",
  "search",
  "tel",
  "text",
  "url",
]);

export const ownsTextEditingKeys = (target: EventTarget | null) =>
  (target instanceof HTMLInputElement && textInputTypes.has(target.type)) ||
  target instanceof HTMLTextAreaElement ||
  (target instanceof HTMLElement && target.isContentEditable);

// Widgets whose own arrow-key behaviour is the point of them: react-aria
// sliders, number fields, listboxes, menus and tab lists all move a value or a
// highlight with the arrows. Matching the ancestors too catches the composed
// parts (an option inside a listbox, a thumb inside a slider) that receive the
// key event instead of the widget root.
const arrowKeyRoles = [
  "combobox",
  "listbox",
  "menu",
  "option",
  "slider",
  "spinbutton",
  "tab",
];
const arrowKeyTargets = [
  "select",
  ...arrowKeyRoles.map((role) => `[role="${role}"]`),
].join(",");

export const ownsArrowKeys = (target: EventTarget | null) =>
  target instanceof HTMLElement &&
  // A range input carries the slider role implicitly, so it needs its own test.
  ((target instanceof HTMLInputElement && target.type === "range") ||
    target.closest(arrowKeyTargets) !== null);
