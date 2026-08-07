import { ResizeDirection } from "./types";

const boundaryMap: Record<
  ResizeDirection,
  { rotation: number; type: "corner" | "edge" }
> = {
  bottom: { rotation: 180, type: "edge" },
  bottomLeft: { rotation: 270, type: "corner" },
  bottomRight: { rotation: 180, type: "corner" },
  left: { rotation: 270, type: "edge" },
  right: { rotation: 90, type: "edge" },
  top: { rotation: 0, type: "edge" },
  topLeft: { rotation: 0, type: "corner" },
  topRight: { rotation: 90, type: "corner" },
};

export function Boundary({ direction }: { direction: ResizeDirection }) {
  const { rotation, type } = boundaryMap[direction];

  return (
    <svg
      aria-hidden
      className="absolute inset-0"
      style={{ transform: `rotate(${String(rotation)}deg)` }}
      viewBox="0 0 100 100"
    >
      {type === "edge" ? (
        <rect className="fill-content-fg/10" height="50" width="100" />
      ) : (
        <path
          className="fill-content-fg/10"
          d="M 0 0 H 100 V 50 H 50 V 100 H 0 Z"
        />
      )}
    </svg>
  );
}
