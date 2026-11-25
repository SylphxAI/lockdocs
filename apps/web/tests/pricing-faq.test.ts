import assert from "node:assert/strict";
import { test } from "node:test";
import { faqPage } from "../src/lib/faq-schema";
import { pricingFaq } from "../src/lib/pricing-faq";

const origin = "https://lockdocs.sylphx.com";
const cases = [
  { locale: "en", path: "/pricing", price: "€109", expected: [
    ["How much does Lockdocs Pro cost?", "€109 per year, including tax"],
    ["Do I need an account to use Lockdocs Free?", "free to use without an account"],
    ["Does Lockdocs upload my source code or private registry credentials?", "scans your project locally"],
    ["How do I activate Pro, and does it work offline?", "checks it offline until it expires"],
    ["What does a Pro Upgrade Report show?", "exact call sites (file:line)"],
    ["Where can I get an invoice or stop my subscription from renewing?", "14 days from purchase"],
  ] },
  { locale: "zh-TW", path: "/zh-TW/pricing", price: "HK$938", expected: [
    ["Lockdocs Pro 要多少錢？", "年費為 HK$938，已含稅"],
    ["使用免費版需要帳戶嗎？", "都免費，也不用登入"],
    ["Lockdocs 會上傳我的原始碼或私有套件庫憑證嗎？", "只在本機掃描專案"],
    ["如何啟用 Pro？離線也能用嗎？", "在到期前離線驗證"],
    ["Pro 升級報告會列出哪些資訊？", "確切的呼叫位置（檔案:行號）"],
    ["在哪裡可以下載發票或停止續訂？", "購買後 14 天內取消"],
  ] },
] as const;

for (const { locale, path, price, expected } of cases) {
  test(locale + " pricing FAQ maps all six localized answers to its canonical FAQPage", () => {
    const items = pricingFaq(locale, price);
    const schema = faqPage(origin + path, items);
    assert.equal(schema["@context"], "https://schema.org");
    assert.equal(schema["@type"], "FAQPage");
    assert.equal(schema["@id"], origin + path + "#faq");
    assert.equal(schema.mainEntity.length, 6);
    for (const [index, [question, answerExcerpt]] of expected.entries()) {
      const entity = schema.mainEntity[index];
      assert.equal(entity["@type"], "Question");
      assert.equal(entity.name, question);
      assert.deepEqual(entity.acceptedAnswer, { "@type": "Answer", text: items[index].answer });
      assert.ok(entity.acceptedAnswer.text.includes(answerExcerpt));
    }
  });
}
