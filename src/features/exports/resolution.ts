// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ExportArtifact } from "./types";

export const cameraResolutionScales = [100, 75, 50];

export const sourceScalePercent = (
  artifact: Extract<ExportArtifact, { kind: "recording" }>,
) => Math.min(400, Math.max(100, artifact.sourceScalePercent));

export const resolutionScales = (
  artifact: Extract<ExportArtifact, { kind: "recording" }>,
) => {
  if (artifact.primaryKind === "audio") return [100];
  if (artifact.primaryKind === "camera") return cameraResolutionScales;
  const source = sourceScalePercent(artifact);
  return [source, 200, 150, 100].filter(
    (scale, index, choices) =>
      scale <= source && choices.indexOf(scale) === index,
  );
};

export const scaledVideoDimensions = ({
  height,
  scale,
  sourceScale,
  width,
}: {
  height: number;
  scale: number;
  sourceScale: number;
  width: number;
}) => {
  const even = (value: number) => Math.max(2, Math.floor(value / 2) * 2);

  return {
    height: even((height * scale) / sourceScale),
    width: even((width * scale) / sourceScale),
  };
};

export const scaledDimensions = (
  artifact: Extract<ExportArtifact, { kind: "recording" }>,
  scale: number,
) => {
  const source = sourceScalePercent(artifact);
  return scaledVideoDimensions({
    height: artifact.height,
    scale,
    sourceScale: source,
    width: artifact.width,
  });
};
