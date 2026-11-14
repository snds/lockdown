import { AppFrameTopRow, AppLockup, FloatButton, TopBar, useAppFrame, type NavMarkProp } from "@unicron/ui"

import { place, usePlaces } from "./places"

interface TopRowProps {
  /** Leaves a pushed place (Search, Sharing, Settings) for the section the person came from. */
  onLeave: () => void
  onLock: () => void
  /** The update mark on the way into Settings (cross-app decisions §3); found-5 fills it. */
  settingsMark?: NavMarkProp
}

/**
 * The phone's top row (brief 3.3, Mobile / 01, 14 and 16). On a section's
 * first screen: the lockup, and the profile button that opens Settings.
 * Once the page has scrolled the frame keeps the mark, shows the place as
 * a small centred title (the switcher, where the head is one) and draws a
 * hairline. A pushed screen gets the top bar instead: back, the title once
 * its own head has scrolled away, and on Settings the Lock button.
 */
export function TopRow({ onLeave, onLock, settingsMark }: TopRowProps) {
  const frame = useAppFrame()
  const { nav, go } = usePlaces()
  if (!frame.phone) return null
  const lock = <FloatButton label="Lock" icon="locked" size="l" onClick={onLock} />
  if (frame.detailOpen) {
    // An open item, or an open Settings section: back to its list.
    return (
      <AppFrameTopRow>
        <TopBar onBack={frame.back?.onBack} backLabel={frame.back?.label} actions={nav.area === "settings" ? lock : undefined} />
      </AppFrameTopRow>
    )
  }
  if (nav.area === "settings")
    return (
      <AppFrameTopRow>
        <TopBar onBack={onLeave} title={place("settings")?.label} titleAs="h1" actions={lock} />
      </AppFrameTopRow>
    )
  if (nav.area === "search" || nav.area === "sharing")
    return (
      <AppFrameTopRow>
        <TopBar onBack={onLeave} />
      </AppFrameTopRow>
    )
  return (
    <AppFrameTopRow
      lockup={<AppLockup name="Lockdown" />}
      switcher={frame.head?.switcher}
      actions={<FloatButton label="Settings" icon="user--avatar" size="l" mark={settingsMark} onClick={() => go("settings")} />}
    />
  )
}
