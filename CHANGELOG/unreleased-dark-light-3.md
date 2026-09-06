- Reading the system light/dark appearance no longer initializes AppKit and CoreUI on
  macOS: the `dark-light` dependency moves from 0.2 to 3, which reads
  `AppleInterfaceStyle` from `NSUserDefaults` instead, and drops the unmaintained `objc`
  crate from the tree. A system that reports no preference resolves to the light scheme,
  as it did before.
