# Model named actions as typed capabilities

Each named action is its own typed descriptor with associated input and output types, CLI-free command metadata, and an independently implemented adapter capability. Actions are not variants of a Resource-wide enum dispatched by strings; the Standout integration renders each declared action as an ordinary subcommand while adapters translate its distinct contract.
