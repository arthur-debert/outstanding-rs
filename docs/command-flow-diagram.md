# Standout Command Flow Diagram

This diagram illustrates how a shell command input string is transformed through the standout framework.

```mermaid
flowchart TB
    subgraph Entry["Entry Point"]
        CLI["Shell Command<br/>Vec&lt;String&gt;"]
    end

    subgraph Parsing["Clap Parsing Stage"]
        AUG["augment_command()<br/>Injects --output, --output-file-path"]
        CLAP["clap::Command::try_get_matches_from()"]
        SEL["Default command<br/>No subcommand selected?<br/>Insert it and parse again"]
        AM["ArgMatches"]
        AUG --> CLAP --> SEL --> AM
    end

    subgraph Routing["Dispatch Routing"]
        ECP["extract_command_path()<br/>→ Vec&lt;String&gt;"]
        JOIN["path.join('.')<br/>→ String"]
        LOOKUP["commands.get(&path_str)<br/>→ DispatchFn"]
        ECP --> JOIN --> LOOKUP
    end

    subgraph PreHook["Pre-Dispatch Hooks"]
        PRE["PreDispatchFn<br/>(ArgMatches, &mut CommandContext)"]
        CTX["CommandContext<br/>{path, app_state, extensions}"]
        PRE --> CTX
    end

    subgraph Handler["Handler Execution"]
        HANDLER["Handler Function<br/>(ArgMatches, CommandContext) → HandlerResult&lt;T&gt;"]
        OUTPUT["Output&lt;T: Serialize&gt;<br/>Render(T) | Silent | Binary"]
        HANDLER --> OUTPUT
    end

    subgraph Serialize["Serialization"]
        SER["serde_json::to_value()"]
        JSON["serde_json::Value"]
        SER --> JSON
    end

    subgraph PostHook["Post-Dispatch Hooks"]
        POST["PostDispatchFn<br/>(ArgMatches, CommandContext, Value) → Value"]
    end

    subgraph RenderDispatch["Render Dispatch"]
        MODE{"Representation?"}

        subgraph Structured["Structured Encodings"]
            STRUCT_SER["Direct Serialization"]
            JSON_OUT["Json: serde_json::to_string_pretty()"]
            YAML_OUT["Yaml: serde_yaml::to_string()"]
            OTHER_OUT["Csv: flat records, or a CsvProjection"]
            NDJSON_OUT["Ndjson: one compact record per line"]
        end

        subgraph HumanText["Human Representation"]
            subgraph Pass1["Pass 1: Template Engine"]
                JINJA["MiniJinjaEngine::render_template()<br/>Template + Value → String"]
                TAGS["String with [style]tags[/style]"]
                JINJA --> TAGS
            end

            subgraph Pass2["Pass 2: Style Processing"]
                BB["BBParser::parse()"]
                TRANSFORM{"TagTransform?"}
                ANSI["Apply → ANSI escape codes"]
                PLAIN["Remove → Plain text"]
                KEEP["Keep → Tags visible"]
            end
        end
    end

    subgraph Result["Render Result"]
        RR["RenderResult<br/>{formatted: String, raw: String}"]
        DO["DispatchOutput<br/>Text{formatted, raw} | Binary | Silent"]
        RR --> DO
    end

    subgraph PostOutput["Post-Output Hooks"]
        PO["PostOutputFn<br/>(ArgMatches, CommandContext, RenderedOutput) → RenderedOutput"]
    end

    subgraph Final["Final Output"]
        RUN["DispatchResult<br/>Handled(String) | Binary | Silent"]
        PRINT["stdout, the named output file, or the pager"]
        RUN --> PRINT
    end

    CLI --> AUG
    AM --> ECP
    LOOKUP --> PRE
    CTX --> HANDLER
    OUTPUT --> SER
    JSON --> POST
    POST --> MODE

    MODE -->|"Json/Yaml/Csv/Ndjson"| STRUCT_SER
    STRUCT_SER --> JSON_OUT
    STRUCT_SER --> YAML_OUT
    STRUCT_SER --> OTHER_OUT
    JSON_OUT & YAML_OUT & OTHER_OUT & NDJSON_OUT --> DO

    MODE -->|"Human/TermDebug"| JINJA
    TAGS --> BB
    BB --> TRANSFORM
    TRANSFORM -->|Apply| ANSI
    TRANSFORM -->|Remove| PLAIN
    TRANSFORM -->|Keep| KEEP
    ANSI & PLAIN & KEEP --> RR

    DO --> PO
    PO --> RUN

    style Entry fill:#e1f5fe
    style Parsing fill:#f3e5f5
    style Routing fill:#fff3e0
    style PreHook fill:#e8f5e9
    style Handler fill:#fce4ec
    style Serialize fill:#fff8e1
    style PostHook fill:#e8f5e9
    style RenderDispatch fill:#e3f2fd
    style Result fill:#f1f8e9
    style PostOutput fill:#e8f5e9
    style Final fill:#ffebee
```

## Type Flow Summary

| Stage | Input Type | Output Type |
| ------- | ----------- | ------------ |
| Entry | `Vec<String>` (CLI args) | - |
| Parsing | `Vec<String>` + `clap::Command` | `ArgMatches` |
| Routing | `ArgMatches` | `DispatchFn` lookup |
| Pre-Hooks | `(&ArgMatches, &mut CommandContext)` | Modified `CommandContext` |
| Handler | `(&ArgMatches, &CommandContext, &mut Results<E>)` | `Result<Output<T>, Error>`, plus each emitted event |
| Serialization | `Output<T>` | `serde_json::Value` |
| Post-Hooks | `Value` | Transformed `Value` |
| Render (Structured) | `Value` + `Representation` | Formatted string (JSON/YAML/etc) |
| Render (Text Pass 1) | Template + `Value` | String with style tags |
| Render (Text Pass 2) | Tagged string + `TagTransform` | ANSI/plain/debug string |
| Result | `RenderResult` | `DispatchOutput` |
| Post-Output | `RenderedOutput` | Transformed `RenderedOutput` |
| Final | `DispatchResult` | stdout, the named output file, or the pager |

## Key Components

- **standout/cli/app.rs** - Entry point (`App::dispatch_from`)
- **standout/cli/core.rs** - Command augmentation
- **standout/cli/dispatch.rs** - Dispatch logic and render orchestration
- **standout-render/template/engine.rs** - MiniJinja template engine
- **standout-render/template/functions.rs** - `apply_style_tags()`, render functions
- **standout-bbparser** - Style tag to ANSI conversion
