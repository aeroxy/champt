// YourImageShare's upload button. Files go from the browser straight to
// YourImageShare, without an account, and Champt only ever sees the link
// that comes back. The widget's box shows each uploader a link to delete
// their file, which Champt never does.

import { useEffect, useRef } from 'react'

/** What the widget reports for a finished upload. */
export type Upload = {
  url: string
  type: 'image' | 'video'
  /** A small preview, when YourImageShare has one yet. */
  thumb: string | null
}

/** The widget's script, as the documented `w.js` snippet loads it: a new version each hour. */
const WIDGET = 'https://yourimageshare.com/upload-widget.js'

/**
 * The button and drop zone, drawn by the widget in a box of its own. The
 * widget sets up the buttons on the page when its script runs, and only
 * then, so it runs again for each new button; it skips the ones it has set
 * up before. Only someone about to post ever loads it. `onUpload` should
 * keep its identity, like a state setter: a new one sets the button up again.
 */
export function UploadButton({ onUpload }: { onUpload: (upload: Upload) => void }) {
  const box = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const host = box.current
    if (!host) return
    // Made here rather than by React, because the widget moves it into a card of its own.
    const link = document.createElement('a')
    link.className = 'yis-upload'
    link.href = 'https://yourimageshare.com/'
    link.textContent = 'Image and video hosting by YourImageShare'
    host.append(link)

    const uploaded = (event: Event) => {
      const item = (event as CustomEvent<{ item?: Partial<Upload> }>).detail?.item
      if (item?.url && (item.type === 'image' || item.type === 'video')) {
        onUpload({ url: item.url, type: item.type, thumb: item.thumb ?? null })
      }
    }
    host.addEventListener('yis:upload', uploaded)

    const script = document.createElement('script')
    script.src = `${WIDGET}?v=${Math.floor(Date.now() / 3_600_000)}`
    script.async = true
    document.head.append(script)

    return () => {
      host.removeEventListener('yis:upload', uploaded)
      host.replaceChildren()
      script.remove()
    }
  }, [onUpload])

  return <div ref={box} />
}
