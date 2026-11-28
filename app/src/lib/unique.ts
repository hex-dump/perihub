/** The first of each item that has the same key as another: a list shown with keyed {#each} blocks must not
 * hold two with one key (Svelte stops drawing the page with each_key_duplicate). */
export function uniqueBy<T>(items: readonly T[], key: (item: T) => string): T[] {
  const seen = new Set<string>();
  return items.filter((item) => {
    const k = key(item);
    if (seen.has(k)) return false;
    seen.add(k);
    return true;
  });
}
