export function cookie(request: Request, name: string) {
  for (const part of (request.headers.get("cookie") ?? "").split(";")) {
    const entry = part.trim();
    if (entry.startsWith(name + "=")) return entry.slice(name.length + 1);
  }
}
export function setPrivateCookie(response: Response, name: string, value: string, options: {maxAge: number}) {
  response.headers.append("Set-Cookie", name + "=" + value + "; Max-Age=" + options.maxAge + "; Path=/; Secure; HttpOnly; SameSite=Lax");
}
