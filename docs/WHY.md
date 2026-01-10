The last few years has seen an evolution a of command  line applications. Moving away from the core Unix tool design, shell apps have been deployed to complex, interactive use cases as in gh's cli, gcloud, lazygit and many others. Part of this has been tooling, an ecosystem of libraries and code that lower the effort for writing complex apps. For example, the raise of command line parsing libs such as python's clicky and rust's clap has freed developer from that work, and has virtually ended prior practices of parsing strings command all of the place, often intermingled with the logic code. 





# outstanding

Outstanding is shell rendering library that allows your to develop your application to be shell agnostic, being unit tested and easier to write and maintain. Likewise it decouples the rendering from the model, giving you a interface that is easier to fine tune and update.

We've been pretty good at not mixing arg parsing and application logic for a while, with great libs like clasp. Thankfully, you
won't see a logic three modules later that program execution parsing an ad hoc option from the input string.  That can't be said about the output, commonly intermingled with logic, with prints to std out or std mid program and premature conversion of data types to strings.  This makes programs hard to test, maintain and design.

**Outstanding** is a library for rendering your application into terminal, be it plain tech, richer formatting or textual or binary data that helps isolate logic and presentation. It support templates strings, template files and style sheets and is smart about gracefully degrading output to plain text when needed.

![alt text](assets/architecture.svg)

# Installation

```toml
[dependencies]
outstanding = "0.2.2"
```

## Quick Start

