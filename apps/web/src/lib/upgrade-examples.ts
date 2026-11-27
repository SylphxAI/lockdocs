// Bundled examples, not results from a visitor’s project.
export const upgrades = {
  next: {
    name: "Next.js", label: "Next.js 14 → 15", command: "lockdocs upgrade next 15.0.0", change: ["cookies() is now asynchronous", "cookies() 改為非同步函式"],
    path: "app/account/page.tsx:8", before: 'const token = cookies().get("session")', after: 'const token = (await cookies()).get("session")',
    oldSource: "next@14.2.3 dist/client/components/headers.d.ts:13", newSource: "next@15.0.0 dist/server/request/cookies.d.ts:24",
    guide: ["Next.js 15 migration · Async request APIs", "Next.js 15 升級指南：非同步請求 API"],
    guideUrl: "https://nextjs.org/docs/app/guides/upgrading/version-15#async-request-apis-breaking-change",
    note: ["Make the containing Server Component async. Next.js 15 provides temporary synchronous compatibility, but the declared return value is now a Promise.",
      "請在呼叫 cookies() 的伺服器元件加上 async。Next.js 15 暫時仍支援同步呼叫，但型別宣告的回傳值已改為 Promise。"],
  },
  pydantic: {
    name: "Pydantic", label: "Pydantic 1.10 → 2.9", command: "lockdocs upgrade pydantic 2.9.2", change: ["Use model_dump() in Pydantic v2", "Pydantic v2 改用 model_dump()"],
    path: "app/serializers.py:14", before: "payload = account.dict()", after: "payload = account.model_dump()",
    oldSource: "pydantic@1.10.18 pydantic/main.py:427", newSource: "pydantic@2.9.2 pydantic/main.py:352",
    guide: ["Pydantic v2 migration · BaseModel changes", "Pydantic v2 升級指南：BaseModel 的變更"],
    guideUrl: "https://docs.pydantic.dev/2.9/migration/#changes-to-pydanticbasemodel",
    note: ["The old dict() method still works in v2, but is deprecated. Replace it with model_dump() at the call site.",
      "v2 仍可使用舊的 dict()，但這個方法已棄用。請將呼叫處改成 model_dump()。"],
  },
  zod: {
    name: "Zod", label: "Zod 3.23 → 4.6", command: "lockdocs upgrade zod 4.6.7", change: ["Use z.email() in Zod v4", "Zod v4 改用 z.email()"],
    path: "src/validation/account.ts:7", before: "const email = z.string().email()", after: "const email = z.email()",
    oldSource: "zod@3.23.8 lib/types.d.ts:200", newSource: "zod@4.6.7 src/v4/classic/schemas.ts:710–712",
    guide: ["Zod v4 migration · String formats", "Zod v4 升級指南：字串格式"],
    guideUrl: "https://zod.dev/v4/changelog#deprecates-email-etc",
    note: ["Zod v4 still supports z.string().email(), but deprecates it in favor of the top-level z.email() validator.",
      "Zod v4 仍支援 z.string().email()，但已將它標記為棄用。請改用頂層的 z.email() 驗證函式。"],
  },
  react: {
    name: "React", label: "React 18 → 19", command: "lockdocs upgrade react-dom 19.0.0", change: ["React 19 removes ReactDOM.render()", "React 19 移除了 ReactDOM.render()"],
    path: "src/main.tsx:6", before: "ReactDOM.render(<App />, container)", after: "createRoot(container).render(<App />)",
    oldSource: "react-dom@18.3.1 cjs/react-dom.development.js:29908", newSource: "react-dom@19.0.0 cjs/react-dom-client.development.js:24452, 24602",
    guide: ["React 19 upgrade guide · ReactDOM.render", "React 19 升級指南：ReactDOM.render"],
    guideUrl: "https://react.dev/blog/2024/04/25/react-19-upgrade-guide#removed-reactdom-render",
    note: ["Import createRoot from react-dom/client. Create one root for the container and call render() on that root.",
      "從 react-dom/client 匯入 createRoot，為容器建立一個根節點，再呼叫該根節點的 render()。"],
  },
};

export type UpgradeId = keyof typeof upgrades;
export const upgradeIds = Object.keys(upgrades) as UpgradeId[];
// The Pro decision proof cites Next.js, so the "What Pro shows you" section opens on another example.
export const decisionProofUpgrade = "next" satisfies UpgradeId;
export const showcaseUpgrade: UpgradeId = "pydantic";

export function upgradeExample(search: string) {
  const values = new URLSearchParams(search).getAll("example");
  return values.length === 1 ? upgradeIds.find(id => id === values[0]) : undefined;
}

export function upgradePreviewPath(playgroundPath: string, id: UpgradeId) {
  return playgroundPath + "?example=" + id + "#upgrade";
}

// Links a "package@version path:line" citation to that file in the published package.
// %40 keeps public HTML free of name@version text that Cloudflare mistakes for email.
export function releaseSourceUrl(citation: string) {
  const match = /^(\S+)@([^@\s]+) ([^:\s]+):\d+$/.exec(citation);
  return match ? "https://unpkg.com/" + match[1] + "%40" + match[2] + "/" + match[3] : undefined;
}
