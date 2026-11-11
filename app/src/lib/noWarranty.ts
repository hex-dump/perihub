// Devices whose owner accepted the "No warranty" notice before the first experimental change: a mouse
// by its USB product id, before the first save to a memory format nobody has tested yet, or another
// device by a name such as "dpi:MX Master 3S". Kept in the embedded browser: if it is lost, PeriHub
// asks once more.

const KEY = "perihub.noWarrantyMice";

/** Also kept here, so that without storage PeriHub asks once per session and not before every save. */
const session = new Set<number | string>();

function stored(): (number | string)[] {
  try {
    const list: unknown = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    return Array.isArray(list)
      ? list.filter((id): id is number | string => typeof id === "number" || typeof id === "string")
      : [];
  } catch {
    return [];
  }
}

/** The owner of this device (USB product id, or name) already accepted the notice. */
export function noWarrantyAccepted(productId: number | string): boolean {
  return session.has(productId) || stored().includes(productId);
}

export function acceptNoWarranty(productId: number | string) {
  session.add(productId);
  const list = stored();
  if (list.includes(productId)) return;
  try {
    localStorage.setItem(KEY, JSON.stringify([...list, productId]));
  } catch {
    /* storage unavailable: asked again after a restart */
  }
}
