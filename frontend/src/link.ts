// Every thread has a link, /t/<id>. Vercel serves the page for every path,
// so the page reads the link itself, and it keeps the open thread's link in
// the address bar.

/** The path a thread is shared under. */
export const threadPath = (id: number) => `/t/${id}`

/** The thread `path` links to, or null if it isn't a thread's link. */
export function linkedThread(path: string): number | null {
  const match = /^\/t\/([1-9]\d{0,14})\/?$/.exec(path)
  return match ? Number(match[1]) : null
}
