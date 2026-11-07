import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import { App } from './App'
import { startAnalytics } from './analytics'
import { finishSignIn } from './auth'

// Before the first render, so the app starts already signed in; and before
// analytics, so a sign-in token has left the address bar by the time it looks.
finishSignIn()
if (import.meta.env.PROD) startAnalytics()

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
