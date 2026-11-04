export type RowValues = Record<string, string | null>;
/** Preserve NULL, empty string and exact decimals; never convert cell values to JS numbers. */
export function rowChanges(original: RowValues, edited: RowValues): RowValues {
  return Object.fromEntries(
    Object.entries(edited).filter(
      ([key, value]) => Object.hasOwn(original, key) && original[key] !== value,
    ),
  );
}
