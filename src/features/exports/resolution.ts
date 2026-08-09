import { ExportArtifact } from "./types";

export const sourceScalePercent = (
  artifact: Extract<ExportArtifact, { kind: "recording" }>,
) => Math.min(400, Math.max(100, artifact.sourceScalePercent));

export const resolutionScales = (
  artifact: Extract<ExportArtifact, { kind: "recording" }>,
) => {
  const source = sourceScalePercent(artifact);
  return [source, 200, 150, 100].filter(
    (scale, index, choices) =>
      scale <= source && choices.indexOf(scale) === index,
  );
};

export const scaledDimensions = (
  artifact: Extract<ExportArtifact, { kind: "recording" }>,
  scale: number,
) => {
  const source = sourceScalePercent(artifact);
  const even = (value: number) => Math.max(2, Math.floor(value / 2) * 2);

  return {
    height: even((artifact.height * scale) / source),
    width: even((artifact.width * scale) / source),
  };
};
