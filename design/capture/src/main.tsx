// The harness's entry. An extension in-page frame (menu, save prompt,
// passkey sheet) loads this same bundle inside its test page's iframe and
// must get none of the apps' styles, so everything else, the sheet with it,
// loads in a second step.

import { extensionFrameName } from "./states/extension-frame-name"

const frame = extensionFrameName(window.name)
if (frame) void import("./states/extension-frame").then((m) => m.mountExtensionFrame(frame))
else void import("./app-main")
