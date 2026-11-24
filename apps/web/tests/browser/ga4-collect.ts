// A Google tag request can carry one event in its URL or several in a
// line-delimited POST body. In a batch the URL can hold shared fields such as
// dl (page location), while each body line contains overrides for one event.
export function ga4CollectEvents(url: URL, postData: string | null): URLSearchParams[] {
  if (!postData) return [url.searchParams];
  return postData.split(/\r?\n/).filter(Boolean).map(line => {
    const event = new URLSearchParams(url.search);
    for (const [key, value] of new URLSearchParams(line)) event.set(key, value);
    return event;
  });
}
