export type Range = {
  max: number;
  min: number;
};

export const randomInRange = ({ max, min }: Range) =>
  Math.random() * (max - min) + min;
