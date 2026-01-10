The last few years has seen an evolution a of command  line applications. Moving away from the core Unix tool design, shell apps have been deployed to complex, interactive use cases as in gh's cli, gcloud, lazygit and many others. Part of this has been tooling, an ecosystem of libraries and code that lower the effort for writing complex apps. For example, the raise of command line parsing libs such as python's clicky and rust's clap has freed developer from that work, and has virtually ended prior practices of parsing strings command all of the place, often intermingled with the logic code. 

For shell apps that are not interactive, but still benefit from rich output control, we simply do not have good options in the rust ecosystem. Even modern shell apps will resource to printing to stand out and  other shell based assumptions. Worse, as a premature conversion to strings looses finer structured data, that often entails in the loss of actual unit tests in favor of integration tests, that also have to parse back output to infer the data.

The end result is that such applications still feel like the 90's when developing and testing from the input onwards.

# outstanding

Outstanding is shell rendering library that allows your to develop your application to be shell agnostic, easily unit tested and easier to write and maintain. Likewise it decouples the rendering from the model, giving you a interface that is easier to fine tune and update.


![alt text](assets/architecture.svg)

Another way to think about this, is an analogy to web libraries and frameworks in the early days. At first people had to do their own low level http parsing , often in C, in order to interface correctly with the data. Soon came a host of tools, from web servers to specialized modules that did that input parsing dirty work. At that point application developers had an easy, practical and correct way to interact with the request itself, by using the infrastructure's parsing for all the complex bits.

At first, the output was similarly ignores. PHP apps would, much like shell apps today, sprinkle prints throughout the code, resulting in the same issues: hard to test, maintain and keep the presentation and core logic decoupled. With time, a second generation of frameworks emerged, one in which the logic and data layers were isolated, with the final string output to HTML at the very end, mostly with template files.

# Vision

Outstanding aspires to similarly help shell applications, by allowing an application to be developed in pure logic and have a regular rust api , with rich data types and rich data types for return, leaving the cli with the task of formatting the output , but offering tools to make that simple, even for very sophisticated outputs.

There are significant differences, of course. There is no shell equivalent of de facto data stores  like SQL, and hence there is less that it can do offload developers.
But there are areas where it can definitely help: 

1. Giving the current output options (term, text, term-debug, auto) options for structured data such as json. 

    This is incredibly useful, as it makes automating output much easier, which effectively makes a cli work like an api.  
    When you application looks like this: 

        notes view 3 --output=json
        { "title": "Hello"}...

    It's trivial to re use it in other contexts (agents, vscode extensions) and the like.

2. Higher Level Abstractions.

    While allowing users to custom build their outputs and data models as they see fit, there is a clear fact that most such shell programs outputs look like: messages (success, failure, help), or a detailed object view for or listing of them.


    If you take this a bit further and, again, use web frameworks like Django  as an example, a explicit definition of your applications types can give you: 
        - free command parsing configuration (app servers list, app servers add --..., app servers delete) boilerplate leaving users  to only define the logic function.
        - free output handling for objects and list of objects (which could use your core structs, or decorate them with derives or other things) 

    While shell applications are definitely more varied than webapps, the crude structure is surprisingly fitting . See gh issue (list view create...) or gcloud commands and so on.


3. Keeping Outstanding Core clean   

    While convenience and easy adoption is a core value, there are many ways to interface with shell applications. Mostly, the first stage of the application, that does input parsing and then dispatch (by subcommand, arguments, options ) is the core flow orchestrator. In fact that is part of the problem, as these do not venture into the output handling expecting the called code to output the result to stdout directly.  

    Hence, actual adoption of a clean core for outstanding means that a lot of glue code has to be written to manage the input -> arg parsing -> application logic -> result output flow. 
    The solution is write integrators, adapters that handle all the boiler plate for users while keeping core clean. That is what the outstanding-clap create does . With time, according to need,s we can write more of these , but presently, being early days and clap having such a strong adoption in the rust world, that's more than enough.




