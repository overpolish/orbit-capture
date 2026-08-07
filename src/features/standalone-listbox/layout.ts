export const standaloneListboxMaxHeight = 150;

const itemHeight = 28;
const itemGap = 8;
const listboxChrome = 10;

export const initialStandaloneListboxHeight = (itemCount: number) =>
  Math.min(
    Math.max(itemCount, 1) * itemHeight +
      Math.max(itemCount - 1, 0) * itemGap +
      listboxChrome,
    standaloneListboxMaxHeight,
  );
