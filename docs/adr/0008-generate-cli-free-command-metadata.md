# Generate CLI-free command metadata

The Resource derive emits CLI-free command metadata rather than Clap types or executable dispatch. A separate Standout integration converts that metadata into the existing command-description and nested-dispatch machinery at an application-chosen mount path, preserving Resource reuse outside CLIs and avoiding a competing dispatcher.
