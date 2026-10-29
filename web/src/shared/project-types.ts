export interface ProjectSummary {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
}

export interface AudioAssetSummary {
  id: string;
  mediaType: string;
  isSealed: boolean;
  sealReason: "completed" | "interrupted" | null;
  sealedAt: string | null;
  frameCount: number;
  observedDurationMs: number;
  storedBytes: number;
}
