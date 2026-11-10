// Mice whose owner accepted the "No warranty" notice before the first save to a memory format
// nobody has tested yet. Kept in the embedded browser: if it is lost, PeriHub asks once more.

const KEY = "perihub.noWarrantyMice";

/** Also kept here, so that without storage PeriHub asks once per session and not before every save. */
const session = new Set<number>();

function stored(): number[] {
  try {
    const list: unknown = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    return Array.isArray(list) ? list.filter((id): id is number => typeof id === "number") : [];
  } catch {
    return [];
  }
}

/** The owner of this mouse (USB product id) already accepted the notice. */
export function noWarrantyAccepted(productId: number): boolean {
  return session.has(productId) || stored().includes(productId);
}

export function acceptNoWarranty(productId: number) {
  session.add(productId);
  const list = stored();
  if (list.includes(productId)) return;
  try {
    localStorage.setItem(KEY, JSON.stringify([...list, productId]));
  } catch {
    /* storage unavailable: asked again after a restart */
  }
}
