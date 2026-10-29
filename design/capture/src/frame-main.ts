// An extension in-page frame (menu, save prompt, passkey sheet): its own
// page, as `frame.html` is in the extension, loaded by the test page's
// iframe. A separate entry, so the frame gets the extension's two sheets and
// none of the apps' styles: from one shared entry the bundler preloaded the
// apps' sheet into the frame too, which drew the frame in unicron's
// surface colours where it expects its own.

import { extensionFrameName } from "./states/extension-frame-name"
import { mountExtensionFrame } from "./states/extension-frame"

const frame = extensionFrameName(window.name)
if (frame) void mountExtensionFrame(frame)
