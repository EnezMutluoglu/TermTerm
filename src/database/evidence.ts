import type { RowSet } from "./types";
export interface EvidencePart extends Partial<RowSet> {
  id: string;
  title: string;
  note: string;
  status: string;
  message?: string;
}
export interface Evidence {
  panel: string;
  engine: string;
  observedAt: number;
  parts: EvidencePart[];
  schema: string;
  table: string;
}
export const evidenceLabels: Record<string, string> = {
  available: "Veri alındı",
  unknown: "Kanıt yok",
  unsupported: "Kaynak bağlı değil",
  permission_denied: "Yetki eksik",
  partial: "Kısmi sonuç",
  failed: "Okunamadı",
  cancelled: "İptal edildi",
};
export function backupObservation(
  row: Record<string, string | null>,
  maxAgeHours: number,
): { state: string; text: string } {
  if (row.is_damaged === "1" || row.is_damaged === "true")
    return { state: "critical", text: "Hasarlı yedek kaydı" };
  if (row.status && row.status !== "COMPLETED")
    return {
      state: /FAIL|ERROR/.test(row.status) ? "critical" : "warning",
      text: row.status,
    };
  if (row.age_seconds == null || row.age_seconds.trim() === "")
    return { state: "unknown", text: "Tamamlanma yaşı bilinmiyor" };
  const age = Number(row.age_seconds);
  if (!Number.isFinite(age) || age < 0)
    return { state: "unknown", text: "Sunucu zamanı doğrulanamadı" };
  if (!Number.isFinite(maxAgeHours) || maxAgeHours <= 0)
    return { state: "unknown", text: "Geçerli yaş eşiği seçin" };
  return {
    state: age > maxAgeHours * 3600 ? "warning" : "observed",
    text: `${(age / 3600).toLocaleString("tr", { maximumFractionDigits: 1 })} saat · ${age > maxAgeHours * 3600 ? "seçili yaş sınırını aşıyor" : "yaş sınırı içinde; restore doğrulanmadı"}`,
  };
}
export function evidenceRows(part: EvidencePart) {
  return (part.rows ?? []).map((row) =>
    Object.fromEntries(
      (part.columns ?? []).map((col, i) => [col.name.toLowerCase(), row[i]]),
    ),
  );
}

export function transactionObservation(
  row: Record<string, string | null>,
  threshold: number,
) {
  const age =
    row.age_seconds == null || row.age_seconds.trim() === ""
      ? NaN
      : Number(row.age_seconds);
  if (
    !Number.isFinite(age) ||
    age < 0 ||
    !Number.isFinite(threshold) ||
    threshold <= 0
  )
    return { state: "unknown", text: "Transaction yaşı veya eşik bilinmiyor" };
  return {
    state: age >= threshold ? "warning" : "observed",
    text: `${age.toLocaleString("tr")} saniye · ${age >= threshold ? "yaş eşiğini aşıyor" : "seçili yaş eşiğinin altında"}`,
  };
}
