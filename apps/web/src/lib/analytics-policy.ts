export type Choice = "reject" | "analytics" | "all";
export const preferenceKey = "lockdocs.cookie-settings.v1";
export const preferenceLifetime = 180 * 24 * 60 * 60 * 1000;
const restricted = new Set("AT BE BG HR CY CZ DE DK EE ES FI FR GR HU IE IS IT LI LT LU LV MT NL NO PL PT RO SE SI SK GB CH".split(" "));
// Assigned ISO 3166-1 alpha-2 codes (IANA tzdb iso3166.tab). CF special codes such as XX/T1 are not countries.
const countries = new Set("AD AE AF AG AI AL AM AO AQ AR AS AT AU AW AX AZ BA BB BD BE BF BG BH BI BJ BL BM BN BO BQ BR BS BT BV BW BY BZ CA CC CD CF CG CH CI CK CL CM CN CO CR CU CV CW CX CY CZ DE DJ DK DM DO DZ EC EE EG EH ER ES ET FI FJ FK FM FO FR GA GB GD GE GF GG GH GI GL GM GN GP GQ GR GS GT GU GW GY HK HM HN HR HT HU ID IE IL IM IN IO IQ IR IS IT JE JM JO JP KE KG KH KI KM KN KP KR KW KY KZ LA LB LC LI LK LR LS LT LU LV LY MA MC MD ME MF MG MH MK ML MM MN MO MP MQ MR MS MT MU MV MW MX MY MZ NA NC NE NF NG NI NL NO NP NR NU NZ OM PA PE PF PG PH PK PL PM PN PR PS PT PW PY QA RE RO RS RU RW SA SB SC SD SE SG SH SI SJ SK SL SM SN SO SR SS ST SV SX SY SZ TC TD TF TG TH TJ TK TL TM TN TO TR TT TV TW TZ UA UG UM US UY UZ VA VC VE VG VI VN VU WF WS YE YT ZA ZM ZW".split(" "));

export function traceAllowsDefault(trace: string) {
  const lines = trace.split(/\r?\n/).filter(line => line.startsWith("loc="));
  const country = lines.length === 1 ? lines[0].slice(4) : "";
  return countries.has(country) && !restricted.has(country);
}

export function savedChoice(raw: string | null): Choice | null {
  if (!raw) return null;
  try {
    const { choice, expires } = JSON.parse(raw);
    const now = Date.now();
    return (choice === "reject" || choice === "analytics" || choice === "all") &&
      Number.isSafeInteger(expires) && expires > now && expires <= now + preferenceLifetime ? choice : null;
  } catch { return null; }
}

export function consent(choice: Choice) {
  return {
    analytics_storage: choice === "reject" ? "denied" : "granted",
    ad_storage: choice === "all" ? "granted" : "denied",
    ad_user_data: choice === "all" ? "granted" : "denied",
    ad_personalization: "denied",
  } as const;
}
