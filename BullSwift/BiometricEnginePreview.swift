// The on-device Biometric Engine preview was removed as part of the thin-client
// migration: the device no longer computes biometric scores locally. Recovery,
// sleep, strain, and stress are computed server-side from the connected device's
// uploaded sensor frames and read back via the data API. Intentionally left as
// an empty translation unit so the build target's file list is unchanged.

import Foundation
