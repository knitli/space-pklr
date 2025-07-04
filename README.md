# space-pklr

**space-pklr** is a project-in-progress aimed at creating a full-featured renderer for Pkl schemas. It builds on the [schematic](https://github.com/moonrepo/schematic) crate to provide robust support for type annotations, constraints, and idiomatic Pkl formatting.
It renders Rust schematic schema, and anything that can be converted into typed schematic schema (currently Typescript and Json Schema)

## Current Status

This project is still under development, and many core features are incomplete. The heart of the project lies in the `PklSchemaRenderer`, which is designed to translate complex schema types into clean, idiomatic Pkl with full type coverage. Key tasks include:

- Implementing rendering for all supported schema types (`Struct`, `Enum`, `Tuple`, etc.).
- Adding support for constraints, defaults, and deprecated types.
- Fine-tuning options for customization, such as indentation and module naming.
- Expanding integration tests for real-world use cases.

## Features (Planned)

- Renders idiomatic Pkl aligned with the [Pkl Style Guide](https://pkl-lang.org/main/current/style-guide/index.html).
- Supports complex type annotations and constraints, including enums, tuples, and unions.
- Handles deprecated fields and default values gracefully.
- Includes options for customization, such as module/class structures and commenting out optional fields for templates.

See [TODO](TODO.md). 

## Why Pkl?

Pkl is a powerful dynamic language purpose-built for configurations. It provides features like type constraints, lazy evaluation, and conditional generation, making it ideal for managing configurations in large-scale applications.

## Contributing

Contributions are welcome! If you’re interested in helping out, take a look at the current implementation in [`src/new_renderer.rs`](https://github.com/knitli/space-pklr/blob/main/src/new_renderer.rs).

## License

This project is licensed under [Plain MIT](LICENSE.md).