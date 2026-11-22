export function measurementId() {
  const id = process.env.LOCKDOCS_GA4_MEASUREMENT_ID?.trim();
  return id && /^G-[A-Z0-9]{10}$/.test(id) ? id : null;
}
