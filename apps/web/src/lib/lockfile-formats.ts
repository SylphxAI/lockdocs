export const formats = ["package-lock.json", "pnpm-lock.yaml", "yarn.lock", "bun.lock", "Cargo.lock", "uv.lock", "requirements.txt", "go.mod"] as const;
export type Format = (typeof formats)[number];
export function detectLockfileFormat(filename: string): Format | undefined {
  const basename = filename.split(/[\\/]/).at(-1)?.toLowerCase();
  return formats.find(format => format.toLowerCase() === basename);
}
