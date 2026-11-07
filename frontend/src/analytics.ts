// Google Analytics, the same property as Aero's other sites. Unlike theirs,
// it starts from code rather than <head>: a sign-in lands on /auth/aero with
// its token in the address, so the tag waits until finishSignIn() has taken
// it out, and is told the address without query or fragment besides.

const MEASUREMENT_ID = 'G-91SNPT9G03'

declare global {
  interface Window {
    dataLayer: unknown[]
    gtag: (...args: unknown[]) => void
  }
}

export function startAnalytics() {
  window.dataLayer = window.dataLayer || []
  // gtag.js reads `arguments` objects off the data layer, not arrays.
  window.gtag = function gtag() {
    window.dataLayer.push(arguments)
  }
  window.gtag('js', new Date())
  window.gtag('config', MEASUREMENT_ID, { page_location: location.origin + location.pathname })
  const script = document.createElement('script')
  script.async = true
  script.src = `https://www.googletagmanager.com/gtag/js?id=${MEASUREMENT_ID}`
  document.head.append(script)
}
