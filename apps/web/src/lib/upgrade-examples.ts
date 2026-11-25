// Bundled examples, not results from a visitor’s project.
export const upgrades = {
  next: {
    label: "Next.js 14 → 15", command: "lockdocs upgrade next 15.0.0", change: ["cookies() is now asynchronous", "cookies() 改為非同步函式"],
    path: "app/account/page.tsx:8", before: 'const token = cookies().get("session")', after: 'const token = (await cookies()).get("session")',
    oldSource: "next@14.2.3 dist/client/components/headers.d.ts:13", newSource: "next@15.0.0 dist/server/request/cookies.d.ts:24",
    guide: ["Next.js 15 migration · Async request APIs", "Next.js 15 升級指南：非同步請求 API"],
    guideUrl: "https://nextjs.org/docs/app/guides/upgrading/version-15#async-request-apis-breaking-change",
    note: ["Make the containing Server Component async. Next.js 15 provides temporary synchronous compatibility, but the declared return value is now a Promise.",
      "請在呼叫 cookies() 的伺服器元件加上 async。Next.js 15 暫時仍支援同步呼叫，但型別宣告的回傳值已改為 Promise。"],
  },
  pydantic: {
    label: "Pydantic 1.10 → 2.9", command: "lockdocs upgrade pydantic 2.9.2", change: ["Use model_dump() in Pydantic v2", "Pydantic v2 改用 model_dump()"],
    path: "app/serializers.py:14", before: "payload = account.dict()", after: "payload = account.model_dump()",
    oldSource: "pydantic@1.10.18 pydantic/main.py:427", newSource: "pydantic@2.9.2 pydantic/main.py:352",
    guide: ["Pydantic v2 migration · BaseModel changes", "Pydantic v2 升級指南：BaseModel 的變更"],
    guideUrl: "https://docs.pydantic.dev/2.9/migration/#changes-to-pydanticbasemodel",
    note: ["The old dict() method still works in v2, but is deprecated. Replace it with model_dump() at the call site.",
      "v2 仍可使用舊的 dict()，但這個方法已棄用。請將呼叫處改成 model_dump()。"],
  },
};
