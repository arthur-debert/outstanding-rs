Shell  Applications  Rise

The last few years has seen an evolution a of command  line applications. This time tacking high complecity, interactive use cases, moving away from the core single-tool Unix design. Applications such as Github's gh or Google's gcloud are prime examples of this trend, with a rich set of commands, subcommands and interactive flows.  That was the result of a long virtual cicle where tooling evolved (libraries, applications, etc), making building great experiences easier, which in turn drove more adoption and more tooling.  

On the more interactive end, we have great TUI infratructure, with libraries, countless examples and a growing ecosystem.
There is a another class of shell applications that, while being more sophisticated then the old couter parts, follow more of a text output model.  Many of these benefit and crave for a good experience in both ends : the development side, that is a platform that is productive to develop on, and the user side , that is a rich and detailed output , which is half the story.


The Parts

    For these the usage starting pioint, the command invokation is now significantly better, with libraries such as clap or clicky.  Apps no longer need to be fussing around low level string parsing , which would invariably be done in many places, often intermingled with logic, and produce UIs that were expensive to test and change, and hence unlikely to polished and improved. 

    Arg parsing libs end by dispatching the code to handle a given command, passing the structuerd input information along.
    However, from that point on, there has been no real established way, nor in best practices nor libraries, to handle the orchestration from the logic execution and the the final output rendering. 

    How Shell Applications Are Structured: ```python
    :: image src=../assets/shell-app-structure.png width=600px
    
    The end result is that applications are develop much like they were 20 years ago. The logic and output are mixed, which prints statements sprkinkled throughout the code. This makes testing it's output way  harder , which not only decreases tests, but entails in having to reverse engireer the output to assert on the data.


Outstanding

    Outstanding is shell rendering library that allows your to develop your application to be shell agnostic, easily unit tested and easier to write and maintain. Likewise it decouples the rendering from the model, giving you a interface that is easier to fine tune and update.

    Another way to think about this, is an analogy to web libraries and frameworks in the early days. At first people had to do their own low level http parsing , often in C, in order to interface correctly with the data. Soon came a host of tools, from web servers to specialized modules that did that input parsing dirty work. At that point application developers had an easy, practical and correct way to interact with the request itself, by using the infrastructure's parsing for all the complex bits.

    At first, the output was similarly ignores. PHP apps would, much like shell apps today, sprinkle prints throughout the code, resulting in the same issues: hard to test, maintain and keep the presentation and core logic decoupled. With time, a second generation of frameworks emerged, one in which the logic and data layers were isolated, with the final string output to HTML at the very end, mostly with template files.

Vision

    Outanding picks up where arg parsing libraries leave off: provinding a clean and structured way to handle the logic execution, passing that to the output layer for rendering. It offers a suite of tools to make rendering detailed, rich outputs easier.     

    For starters it allows file based templates, with stylesheets , themes and several layout helpers. These combined with stuctured conventions, result in a workflow where developers can focus on the logic core, and have a great experience for users with minimal effort.  No only that, by having a cleanly separated and eforced split between these,  they reap the benefits of a focused and unit testable codebase . At the same time, both the logic and the output can be iterated on and improved independently, resulting in a better overall experience.  

    Just like webapplciations became way more mangeable, capable and faster to build when logic, output and styling were separated,  shell applications  will benefit from the same approach.  Oustanding takes a many great lessons from the web  story, which is way more like this subset of applications thatn we assume.

Outstanding Applications

    We will go over how the library works, it's core concepts and components, and how to use it to build great shell applications. 

    1. The Application Life Cycle

        Oustanding is arg parsing library agnostic. It can be used with any such library, as long as you can get the structured input data to your application logic.. Once is that you define the core logic execution function and the template to use when formatiing it's output.

        The full cicle in the CommandRun, which is a sequence of: 
            1. Pre dispatch hooks (optional)
            2. Core Executtion (Application Logic)
            3. Post dispatch hooks (optional)
            4. Output Rendering
            5. Post output hooks (options).

        Outanding will mangage the full cicle, with configuratble hooks at each step, allowing you to customize the behavior as needed.

        1.1 The Core Logic Execution

            The core logic function is any function that takes structured command input (subcommands, arguments, flags) and resturn structured output data as Results.  This is what the application actually does, the rest being supporting characters, and outstanding job's it to let you focus on it.

            :: notes ::
                - Do double check if there us a type expectation for results and what that is.
                - In the future, we want to formalize a shared core of retsult primitives : ```python
                    - Distinguish between listing and detail results.
                    - Offer a rich Message enum that can represent different types of messages (info, warning, error, success, etc), and can be interwoven in results..
                
        1.2. Output Rendering

            Rich apps tend to benefit from various outputs formats. Outsanding supports mutliple formats, with two groups of output: 
                1.  Unstructured Textual Outputs
                    1.1 Term : rich formatted terminal output.
                    1.2 Plain : plain text output, suitable for piping and redirection.
                    1.3 Auto: the library gracefully degrades between Term and Plain based on the output target (terminal emulators , pipes, files, etc)
                2. Structured Data Outputs: Automatically Generated.
                    For structured data, you can customize serialization options from SERDE derivers.

                    2.1 JSON
                    2.2 YAML  
                    2.3 CSV
                    2.4 XML

                3. Binary Outputs
                    3.1 In this scenario, the output is just a pass through to the result.
                
                4. File Outputs
                    4.1 Commands can be configured to write to the file system directly and can do so by using predefined flags , that is, requiring no code from the application layer.

                That is to say, you define the formatted text output via templates , and outstanding will handle graceful degradation and structured outputs automatically.

    2. Rich Output made Easy

        Outstanding  leverages key ideas from the web app world, albeit a simplified version better suiteed for shell apps.

        2.1 Templates

            Templates are minijinja templates, with some custom extensions and filters to make shell output easier.  They can be file based, or inline strings. During development, file based templates wil be releaded on each render, making fof a faster dev cycle.

            :: note :: 
                - Verify if runtime file based loadng is supported . The advantage ir faster dev cycles.
                - This is likely not the case, 

            Templates are vanilla minijinga,  with the widely known syntax from jinja2 as most fetatures, including a library of helper filters and functions from oustanding and use supplied ones when needed. Templates support includes , meaning that even complex structures can be broken doewn into smaller, more manageable and reusable pieces.  

            For example, it's often useful to have a rendering of a core business object, such as a User or Project, which can be used in various places, ensuring consistency and reducing duplication. 

        2.2 Stylesheets And Themes

            Similar to css,  style( name) in templates will mark that conetent to be injeted with formatting options defined in stylesheets. These can also be references to other styles, allowing them to be composed and used in layers. 

            For example, one usually makrs content bits , in templates, in a semantic way: title, name, age, email. This makes the template readable, and decouples the styling from the content structure. Then the style sheets can either define the formmatting or refer to another styles.  This is useful as various data types may share common styles, and this pattern allows to reusem them easily and specialized them as needed, witouth having to touch rust code at all.

            Styles are console::Style structs and can define the expected attributes: foreground and background colors, text weight, decorations,  italics, etc. Color values can be named corlors or hex / rgb values.

            Oustanding themes are adaptative themes, which can define light and dark modes. Mode application and update is handled by outstainding automatically, based on terminal capabilities .

            Themes can be defined in code or via yaml files, making it easy to define custom themes without code changes. During developmeent, just like templates are hot reloaded, so are themes, making for a fast dev cycle.
    
        2.3 Layout Helpers

            Once formatting is easy and flexible, the last piece of the puzzle is the layout. While text lines require no further work, most complex layout encodes informations vertically as well. Typically, these are laborious and error prone to get right, once you factor in padding, truncation, aligment, variable line widths and escape codes. 

            The tables formatter allows one to define a template configuration and then simply pass a list of items to render them in a columnar view. Thse can be formal tables, with clear separators, headers but also implicit ones that are used for listing like views.

            This format is very expressive: min and max widths, paddings, aligments (to both right and left els), customizable truncation, and expading dynamic columns.  The toolset is powerful enought to handle complex and precise outputs to be formatted declaratively by specifing each columns behaviour.

        With these building blocks: resusable, extensible and composable templates, flexible and adaptative styles and themes and powerful layout helpers all working in tandem with auto releading during development dramatically reduce the time to sophisticated outputs.  When each result takes a few minutes to make perfect, building applications that stand out is way easier.

    The basic pieces of oustading are now in place: a declarative app description that links the parsed inputs to the your appslication's logic , followed by a flexible and rich output layer. For developers, after a few minutes of setup, their time can be focused on the core logic, while offering a world class user experience that used to take a large investiment of time and effort to achieve.

Interopability, Adoption and Coupling  

    Oustanding leverages best-of breed tools: console:Style, minijinja for templates, serde for serialization.  These are universal enough to play well other other libraries, and they make few requirements and assumptions about the rest of the application. 

    The interfaces are design around types, making them interopable with various others tools and libraries,  usually with little to no adaptation needed.

    For the arg parsing library oustanding-core is agnostic. Making it work with any such library.  Additionally, given the prevalence of clap, we provide a clap integration crate, that makes it dead simple to wire it up, while keeping all the clap features and interfaces in place.

    Oustanding can be adopted incrementatlly: both horizontally (for a subcommand, a set fo these, or the full app) and veritically (just the output layer, or the the templating ).  Naturally, as it's parts are designed to work well together, the more an app uses, the more value it can take from it.

    We recognize that adding significan layers for a shell application can be a delicate proposition, which is why oustanding is designed to allow a flexible, incremental adoption path.