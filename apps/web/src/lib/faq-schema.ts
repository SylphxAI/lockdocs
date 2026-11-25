export type Faq = {
  question: string;
  answer: string;
  links?: readonly { href: string; label: string }[];
};

export function faqPage(url: string, items: readonly Faq[]) {
  return {
    "@context": "https://schema.org",
    "@type": "FAQPage",
    "@id": url + "#faq",
    mainEntity: items.map(item => ({
      "@type": "Question",
      name: item.question,
      acceptedAnswer: { "@type": "Answer", text: item.answer },
    })),
  };
}
