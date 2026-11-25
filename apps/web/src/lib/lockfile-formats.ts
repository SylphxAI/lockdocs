export const formats = ["package-lock.json", "bun.lock", "Cargo.lock", "uv.lock", "requirements.txt"] as const;
export type Format = (typeof formats)[number];
