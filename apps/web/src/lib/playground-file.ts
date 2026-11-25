import { detectLockfileFormat, type Format } from "./lockfile-formats";

export const previewLimit = 400_000;
export const offlineCommand = "npx -y @sylphx/lockdocs --offline";

type FileRead = { kind: "ready"; format: Format; text: string } | { kind: "oversize" } | { kind: "unreadable" };

export async function readPreviewFile(file: Pick<File, "name" | "size" | "text">, fallback: Format): Promise<FileRead> {
  if (file.size > previewLimit) return { kind: "oversize" };
  let text: string;
  try { text = await file.text(); }
  catch { return { kind: "unreadable" }; }
  if (text.length > previewLimit) return { kind: "oversize" };
  return { kind: "ready", format: detectLockfileFormat(file.name) ?? fallback, text };
}
