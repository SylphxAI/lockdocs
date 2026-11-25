import { localizedPath, type Locale } from "./locales";

export function proProofPaths(locale: Locale) {
  return {
    preview: localizedPath("/playground", locale) + "#upgrade",
    report: "/pro#upgrade-report",
    privateSources: "/pro#private-sources",
  };
}

export function pricingFaq(locale: Locale, annual: string, carryCampaign = (href: string) => href) {
  const en = locale === "en";
  const paths = proProofPaths(locale);
  return [
    {
      question: en ? "How much does Lockdocs Pro cost?" : "Lockdocs Pro 要多少錢？",
      answer: en ? "Lockdocs Pro costs " + annual + " per year, including tax. You pay the full annual amount upfront."
        : "Lockdocs Pro 年費為 " + annual + "，已含稅，訂閱時一次付清。",
    },
    {
      question: en ? "Do I need an account to use Lockdocs Free?" : "使用免費版需要帳戶嗎？",
      answer: en ? "No. Exact-version docs, symbol lookup and the local MCP server are free to use without an account. You sign in to buy or manage Pro."
        : "不用。查閱指定版本的文件、查詢符號和使用本機 MCP 伺服器都免費，也不用登入。購買或管理 Pro 時才需要登入。",
    },
    {
      question: en ? "Does Lockdocs upload my source code or private registry credentials?" : "Lockdocs 會上傳我的原始碼或私有套件庫憑證嗎？",
      answer: en ? "No. lockdocs upgrade scans your project locally. When private fetching is enabled, Lockdocs downloads packages directly from your configured registry using your existing credentials. It sends those credentials only to the origin they’re configured for."
        : "不會。lockdocs upgrade 只在本機掃描專案。啟用私有套件下載時，Lockdocs 會沿用你現有的憑證，直接從設定的套件庫下載，且只會將憑證傳給原先設定的來源。",
    },
    {
      question: en ? "How do I activate Pro, and does it work offline?" : "如何啟用 Pro？離線也能用嗎？",
      answer: en ? "Sign in and subscribe. Then run lockdocs login on your machine and approve access in the browser using the account you bought Pro with. The CLI stores a signed licence and checks it offline until it expires. Offline use relies on installed or cached files; new downloads still need a connection."
        : "登入並訂閱後，在本機執行 lockdocs login，再用購買 Pro 的帳戶在瀏覽器中授權。CLI 會儲存已簽署的授權憑證，並可在到期前離線驗證。離線時可使用已安裝或快取的檔案，下載新檔案仍需連線。",
    },
    {
      question: en ? "What does a Pro Upgrade Report show?" : "Pro 升級報告會列出哪些資訊？",
      answer: en ? "It shows which API changes affect your project, the exact call sites (file:line), and the before-and-after signatures where both exist, with citations to the pinned releases’ declarations."
        : "報告會列出影響專案的 API 變更與確切的呼叫位置（檔案:行號）。如果新舊版本都有宣告，會並列變更前後的函式簽名，並標明各版本的宣告出處。",
      links: [
        { href: carryCampaign(paths.preview), label: en ? "Open the playground" : "開啟試用工具" },
        { href: carryCampaign(paths.report), label: en ? "Read the report reference" : "閱讀升級報告說明（英文）" },
      ],
    },
    {
      question: en ? "Where can I get an invoice or stop my subscription from renewing?" : "在哪裡可以下載發票或停止續訂？",
      answer: en ? "Open the Stripe billing portal from your Lockdocs account to download invoices for reimbursement or tax records and manage renewal. The full year is billed upfront, tax included. UK and EU consumers generally have 14 days from purchase to cancel a new subscription; see the terms for how to cancel and which exceptions apply."
        : "從 Lockdocs 帳戶開啟 Stripe 帳務入口，即可下載發票供公司報帳或稅務留存，也能管理續訂。年費含稅，訂閱時一次付清。英國與歐盟的消費者通常可在購買後 14 天內取消新訂閱；取消方式與例外情況請見訂閱條款。",
      links: [
        { href: "/dashboard", label: en ? "Open your account" : "開啟帳戶" },
        { href: "/legal/terms", label: en ? "Read the subscription terms" : "閱讀訂閱條款" },
      ],
    },
  ];
}
