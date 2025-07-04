/**========================================================================
 * *                              About
 *
 *   (c) 2025 Stash AI Inc. (aka Knitli)
 *   Written by Adam Poulemanos [@bashandbone](https://github.com/bashandbone)
 *   Licensed under the [Plain MIT License](https://plainlicense.org/licenses/permissive/mit/) (Tl;dr: do what you want, give credit, assume nothing)
 *   moonrepo, Inc. created and maintains moon and schematic, under the
 *   (traditional) MIT license. I don't know them, they seem nice.
 *
 *========================================================================**/
//! =========================================================================
//!                           # PklSchemaRenderer
//! =========================================================================
//! A full-service [schematic](https://moonrepo.github.io/schematic/) `SchemaRenderer` for Pkl.
//!
//! Schematic already has a [Pkl *template* renderer](https://github.com/moonrepo/schematic/blob/master/crates/schematic/src/schema/renderers/pkl_template.rs), but it only provides basic rendering for template/template generation. This implementation produces robustly typed schemas with type annotations, constraints, defaults, nuanced type handling, and idiomatic Pkl constructs.
//!
//! ## Why?
//!
//! Pkl offers a powerful schema system, with an exceptionally robust type system. This makes it ideal for configuration management in large repos and organizations. By opening the door to direct schema generation, you can now write configurations based on those schema that:
//! - Have first-class IDE support, providing as-you-type type information, usage tips, documentation, syntax linting/highlighting, and more. See [the Pkl tools docs](https://pkl-lang.org/main/current/tools.html).
//! - Can force schema, default, and config alignment across large repos. Pkl's powerful `extend`/`amend` capabilities allow you to *treat the root config as an enforced type*, and, if you allow it, allow people to make reasonable changes to defaults. A single source of truth with built-in flexibility.
//! - Pkl itself is a powerful dynamic language. It is purpose-built for configurations. You can use its dynamic configs directly, or to generate conditional configs for any common format (yaml, toml, json, messagepack...). Or, use your Pkl to generate native Pkl static configs, `pcf`. (Syntax-wise, it feels closest to `Swift`)
//! - The pkl language has sophisticated capabiilties you won't find in config formats -- for and when generators, complex conditionals, built-in converters, a lazy-evaluation-by-default framework.
//!
//! Bottom line: it's pretty cool.
//!
//! ## Key Features and Design Notes
//!
//! The renderer aims to:
//! - Render idiomatic Pkl aligned to the [Pkl Style Guide](https://pkl-lang.org/main/current/style-guide/index.html) by default. There are some options that allow you to customize away from that default benchmark, but I wanted to deliver uncompromising pkl.
//! - Provide robust type annotations and constraints, including:
//!   - Full type coverage for deeply nested, complex, and optional types.
//!   - Full use of Pkl's type system -- even including [`DataSize`](https://pkl-lang.org/main/current/language-reference/index.html#data-sizes) and [`Duration`](https://pkl-lang.org/main/current/language-reference/index.html#durations) if correctly marked by schematic.
//!   - Complete implementation of schematic's available type constraints. Pkl's type system allows arbitrary constrained types. This is a valid type in Pkl:
//!     ```pkl
//!
//!    /// self-validating email type -- valid pkl
//!    typealias Email = String(
//!       matches(
//!         Regex(
//!           #"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$"#
//!         )
//!        )
//!       )
//!
//!     // and so is:
//!
//!     /// You could also define this long anonymous function in a
//!     /// separate named function and just call it in the annotation.
//!     /// You could also define it inline without defining an alias.
//!     typealias UserData: Mapping<String, Listing<String>>(
//!       List("email", "address", "id")
//!         .every(
//!           (k) -> this.keys.containsKey(k)) && // required keys present
//!         this.every(
//!           (k,v) -> !k.isEmpty && //no empty keys
//!             !v.isEmpty &&                   // no empty values
//!             if (k == "email"))
//!               v.every(
//!                 (email) -> email is Email   // all valid emails
//!               ) &&
//!               v.isDistinct                  // all emails are unique
//!       )
//!
//!     class Customers {
//!       users: UserData
//!       product: AcmeType
//!     }
//!     ```
//!   (The example is intentionally over-the-top, but hopefully you see why this helps make Pkl a powerful configuration language.)
//!
//!   - Handle complex types like `Struct`, `Array`, `Object`, `Tuple`, and `Union` with full type annotations and constraints.
//!   - Support enum translations as type aliases or literal unions, with full type annotations.
//!   - Allow for including or excluding (default) deprecated types. Included deprecations use Pkl's `@Deprecated` decorator with reason and `since` version if available from schematic.
//!   - Correct marking of default values, such as with the `*` operator.
//!   - Support for `open` classes/modules, enabling Pkl's `extend` and `amend` features.
//!   - Renders the top-level `Config` struct as a module by default, but can be switched to a class. This allows you to directly use the generated module as a type using `amends`.
//!   - Customizable options for module/class naming, indentation, and more.

/**========================================================================
 **                       ## A Crash Course in schematic
 **========================================================================
 **       (You can skip this if you're not going to work on the Renderer)
 *========================================================================**/
//
//! I'm going to explain this simply because the type structure was hard to understand.
//! This is my `schematic 101`. The [docs](https://moonrepo.github.io/schematic/) are good, they just didn't click for me.
//!
//! # Schematic’s Core Types & Traits
//!
//! - **`Config` (trait)**: Marks types you want to use as configuration roots. The runtime config (e.g., `ConfigSettingMap`) isn’t directly relevant for schema rendering—what matters is the structure of your Rust config types and how they’re described in the schema system.
//!
//! - **`SchemaGenerator`**: The bridge between your Rust types and the schema representation. It recursively walks your config’s type tree and produces an `IndexMap<String, Schema>`, where keys are **names of named types** (structs, enums, unions, and sometimes type aliases) and values are their `Schema` definitions. **Let's call this the `TypeMap`**.
//!
//!   **Key points about the `TypeMap`:**
//!   - It’s flat at the top level: **all named types** from your config (root and nested, at any depth) are present as siblings.
//!   - “Named type” = any type that schematic gives a name to (struct, enum, union, sometimes type alias). Primitives (`String`, `i32`, etc.) and standard generics (`Vec<_>`, `Option<_>`, `HashMap<_, _>`, etc.) are NOT included as top-level entries, unless you’ve defined a named alias and schematic captures that name.
//!   - It **maps type names** (not field/property names) **to their schemas**. Fields are described within the `fields` property of struct or enum schemas.
//!   - The type graph is potentially deep/complex: schemas reference each other, creating nested structures and supporting recursion.
//!   - The `SchemaGenerator` creates this map and hands it to a `SchemaRenderer`.
//!
//! - **`SchemaRenderer` (trait)**: Takes the `TypeMap` and translates it into your target format (e.g., Pkl, TypeScript, JSON Schema, etc.). **The renderer is the translator.**
//!
//! ## `Schema` and `SchemaType`
//!
//! `Schema` represents a single named type in the `TypeMap`. Its most important field is `ty: SchemaType`, which describes the actual type.
//!
//! ```text
//! Schema
//!  ├── deprecated: Option<String>       // marked deprecated?
//!  ├── description: Option<String>      // doc comment/description
//!  ├── name: Option<String>             // type name (if any)
//!  ├── nullable: bool                   // nullable?
//!  └── ty: SchemaType                   // ← **This is the important part**
//!      └── Struct(Box<StructType>)      // (could be any SchemaType variant)
//!          ├── fields: IndexMap<String, Box<SchemaField>>
//!          │   ├── "field1" -> SchemaField
//!          │   │   └── schema: Schema
//!          │   │       └── ty: SchemaType::String | Enum | Struct | ...
//!          │   └── "field2" -> SchemaField
//!          │       └── schema: Schema
//!          │           └── ty: SchemaType::...
//!          └── partial: bool
//! ```
//!
//! **`SchemaType`** is an enum describing what kind of type the schema represents:
//!
//! ```rust,ignore
//! pub enum SchemaType {
//!   Null, Unknown,
//!   Array(Box<ArrayType>), Boolean(Box<BooleanType>), Enum(Box<EnumType>),
//!   Float(Box<FloatType>), Integer(Box<IntegerType>), Literal(Box<LiteralType>),
//!   Object(Box<ObjectType>), Struct(Box<StructType>), String(Box<StringType>),
//!   Tuple(Box<TupleType>), Union(Box<UnionType>),
//!   Reference(String),  // ← This one's special
//! }
//! ```
//!
//! **`Reference`** is special. It’s a pointer to another named type **by name**, rather than by inlining the whole definition. In Rust, this happens when a struct (or enum/union) field uses a named type:
//!
//! ```rust,ignore
//! pub struct Heroes {
//!   marvel: Marvel, // ← `Marvel` is a `Reference` here
//!   dc: DC,         // ← `DC` is also a `Reference`
//! }
//! ```
//! References enable recursion (e.g., linked lists), sharing types, and keeping the type graph acyclic for rendering (that's arcane programmer-speak for 'no infinite loops').
//!
//! # How to introspect `SchemaType` variants
//!
//! - **`Struct`**: Access its fields via the `fields` property (`IndexMap<String, Box<SchemaField>>`).
//! - **`Array`**: Look at the `items_type` property, which is a `Schema` for the array’s element type.
//! - **`Object`**: Use `key_type` and `value_type` (both `Schema`) for key/value types.
//! - **`Tuple`**: Use `items_types` (a vector of `Schema`) for each tuple slot. (Trick: `Tuple` = multiple `items_types`, `Array` = one `items_type`)
//! - **`Enum`**: Use `values` for C-like literals (`Vec<LiteralValue>`), and `variants` (if present) for struct/tuple variants (`Option<IndexMap<String, Box<SchemaField>>>`).
//! - **`Reference`**: The `String` is the name; look up that named type in the `TypeMap`.
//!

use std::collections::HashSet;
use indexmap::IndexMap;
use schematic::format::Format;
use schematic::schema::{RenderResult, SchemaRenderer, RenderError};
use schematic_types::*;

use crate::constants::{DATA_SIZE_UNITS, DURATION_UNITS};
use crate::types::{TypeMap, EnumTranslation, OpenStructs, ConfigTranslation, OptionalFormat, PropertyDefault, LoadedConfig};

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderType {
    Template,
    #[default]
    Schema,
}

impl std::str::FromStr for RenderType {
  fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
    match s.to_lowercase().as_str() {
      "template" | "tmpl" | "t" => Ok(RenderType::Template),
      "schema" | "sch" | "s" => Ok(RenderType::Schema),
      _ => Err(RenderError::UnsupportedFormat {
        format: s.to_string(),
        available: vec!["template", "schema"],
      }),
    }
  }
}

/// Renders idiomatic Pkl schema definitions with type annotations and constraints.
pub struct PklSchemaRenderer {
    schemas: TypeMap,
    options: PklSchemaOptions,
    depth: usize,
    /// Track typealiases to avoid duplicates
    typealiases: IndexMap<String, String>,
    /// Track `Reference`s to prevent the universe from imploding
    references: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct PklSchemaOptions {
    /// The name of the config to use for the root schema, LoadedConfig (moon config type or one you give); no default
    ///
    pub config_name: LoadedConfig,
    /// Include documentation comments from schema descriptions
    pub include_docs: bool,

    /// Include type constraints where available
    /// Pkl allows for arbitrary type constraints within its types, so constraints will be enforced by Pkl's evaluator. Constraints are limited to those supported by schematic, which vary by type (they include regex pattern, min/max length or number, and required keys).
    pub include_constraints: bool,

    /// are you using this for a template or a schema? Primarily affects case decisions.
    pub render_type: RenderType,

    /// Disable references and render all types inline recursively.
    pub disable_references: bool,

    /// Indentation string (default: 2 spaces)
    pub indent: String,

    pub 

    /// Include default values in the schema
    pub include_defaults: bool,

    /// Include deprecated fields in the schema
    pub include_deprecated: bool,

    /// Whether to comment out optional fields in the schema, useful for template-style generation
    pub comment_out_optional: bool,

    /// A list of properties to exclude from created schema
    pub exclude_properties: Vec<&str>,

    /// A list of imports to add to the generated module. These must be valid `pkl` import paths
    pub added_imports: Vec<&str>,

    /// How to translate enum types (typealias/literal_union; default: typealias)
    pub enum_translation: EnumTranslation,

    /// Whether to mark public structs as `open` when translated to classes (open/no; default: open)
    pub open_structs: OpenStructs,

    /// Whether to render the module as `open module ModuleName` (open/no; default: open)
    pub open_module: OpenStructs,

    /// How to translate the top-level `Config` struct (module/class; default: module)
    pub config_translation: ConfigTranslation,

    /// How to render optional type annotations (optional/optional_explicit_nothing; default: optional)
    pub optional_format: OptionalFormat,

    /// Whether to default to requiring properties or marking them optional when the schema lacks information on optionality.
    pub property_default: PropertyDefault,
}

impl Default for PklSchemaOptions {
    fn default() -> Self {
        Self {
          config_name: LoadedConfig::default(),
          include_docs: true,
          include_constraints: true,
          render_type: RenderType,
          disable_references: false,
          indent: "  ".to_string(),
          include_defaults: true,
          include_deprecated: false,
          comment_out_optional: false,
          exclude_properties: Vec::new(),
          added_imports: Vec::new(),
          enum_translation: EnumTranslation::TypeAlias,
          open_structs: OpenStructs::Open,
          open_module: OpenStructs::Open,
          config_translation: ConfigTranslation::Module,
          optional_format: OptionalFormat::Optional,
          property_default: PropertyDefault::RequireProperties,
        }
    }
}

impl PklSchemaRenderer {
    pub fn new(options: PklSchemaOptions) -> Self {
        Self {
            schemas: IndexMap::default(),
            options,
            depth: 0,
            typealiases: IndexMap::default(),
            references: HashSet::new(),
        }
    }

    pub fn default() -> Self {
        Self::new(PklSchemaOptions::default())
    }

    fn indent(&self) -> String {
        self.options.indent.repeat(self.depth)
    }

    /// Convert to PascalCase for classes and modules
    fn to_pascal_case(&self, name: &str) -> String {
        if name.is_empty() {
            return name.to_string();
        }

        let mut result = String::new();
        let mut capitalize_next = true;

        for ch in name.chars() {
            if ch == '_' || ch == '-' {
                capitalize_next = true;
            } else if capitalize_next {
                result.push(ch.to_uppercase().next().unwrap_or(ch));
                capitalize_next = false;
            } else {
                result.push(ch);
            }
        }

        result
    }

    /// Check if a name is a Pkl keyword and needs escaping
    fn is_pkl_keyword(&self, name: &str) -> bool {
        matches!(
            name,
            "abstract"
                | "amends"
                | "as"
                | "case"
                | "class"
                | "const"
                | "default"
                | "delete"
                | "else"
                | "extends"
                | "external"
                | "false"
                | "fixed"
                | "for"
                | "function"
                | "hidden"
                | "if"
                | "import"
                | "import*"
                | "in"
                | "is"
                | "let"
                | "local"
                | "module"
                | "new"
                | "nothing"
                | "null"
                | "open"
                | "out"
                | "outer"
                | "override"
                | "overrides"
                | "protected"
                | "read"
                | "read*"
                | "record"
                | "super"
                | "switch"
                | "this"
                | "throw"
                | "trace"
                | "true"
                | "typealias"
                | "unknown"
                | "vararg"
                | "when"
        )
    }

    /// Escape a name if it's a keyword
    fn escape_name(&self, name: &str) -> String {
        if self.is_pkl_keyword(name) {
            format!("`{}`", name)
        } else {
            name.to_string()
        }
    }

    fn render_union_default(&self, schema: &Schema) -> String {
        // TODO: Implement union default rendering
        String::new()
    }

    fn set_number_constraints(&self, schema: &Schema) -> String {
        let mut constraints = Vec::new();

        // Extract the number type based on schema type
        let (minimum, maximum, minimum_exclusive, maximum_exclusive, multiple_of) = match &schema.ty {
            SchemaType::Integer(int_type) => (
                int_type.minimum.as_ref(),
                int_type.maximum.as_ref(),
                int_type.minimum_exclusive.as_ref(),
                int_type.maximum_exclusive.as_ref(),
                int_type.multiple_of.as_ref(),
            ),
            SchemaType::Float(float_type) => (
                float_type.minimum.as_ref(),
                float_type.maximum.as_ref(),
                float_type.minimum_exclusive.as_ref(),
                float_type.maximum_exclusive.as_ref(),
                float_type.multiple_of.as_ref(),
            ),
            _ => return String::new(),
        };

        // Min/max constraints (inclusive)
        if let Some(min) = minimum {
            if let Some(max) = maximum {
                constraints.push(format!("isBetween({}, {})", min, max));
            } else {
                constraints.push(format!("this >= {}", min));
            }
        } else if let Some(max) = maximum {
            constraints.push(format!("this <= {}", max));
        }

        // Exclusive min/max constraints
        if let Some(min_ex) = minimum_exclusive {
            constraints.push(format!("this > {}", min_ex));
        }
        if let Some(max_ex) = maximum_exclusive {
            constraints.push(format!("this < {}", max_ex));
        }

        // Multiple of constraint
        if let Some(multiple) = multiple_of {
            constraints.push(format!("this % {} == 0", multiple));
        }

        if !constraints.is_empty() {
            format!("({})", constraints.join(" && "))
        } else {
            String::new()
        }
    }

    fn render_constraints(&self, schema: &Schema) -> String {
        if !self.options.include_constraints {
            return String::new();
        }

        match &schema.ty {
            SchemaType::Integer(int_type) => {
                return self.set_number_constraints(&schema);
            }
            SchemaType::Float(float_type) => {
                return self.set_number_constraints(&schema);
            }
            SchemaType::String(string_type) => {
                let mut constraints = Vec::new();

                // Length constraints
                if let Some(min_len) = &string_type.min_length {
                    if let Some(max_len) = &string_type.max_length {
                        constraints
                            .push(format!("this.length.isBetween({}, {})", min_len, max_len));
                    } else {
                        constraints.push(format!("this.length >= {}", min_len));
                    }
                } else if let Some(max_len) = &string_type.max_length {
                    constraints.push(format!("this.length <= {}", max_len));
                }

                // Pattern constraint
                if let Some(pattern) = &string_type.pattern {
                    constraints.push(format!("matches(Regex(#\"{}\"#))", pattern));
                }

                // Common format-based constraints
                if let Some(format) = &string_type.format {
                    match format.as_str() {
                    "email" => constraints.push("contains(\"@\")".to_string()),
                    "uri" | "url" => constraints.push("startsWith(\"http\")".to_string()),
                    "uuid" => constraints.push("matches(Regex(#\"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$\"#))".to_string()),
                    "ipv4" => constraints.push("matches(Regex(#\"^((25[0-5]|(2[0-4]|1\\d|[1-9]|)\\d)\\.?\\b){4}$\"#))".to_string()),
                    _ => {}
                  }
                }

                // Non-empty constraint for min_length = 1
                if let Some(min_len) = &string_type.min_length {
                    if *min_len == 1 && !constraints.iter().any(|c| c.contains("length")) {
                        constraints.push("!isBlank".to_string());
                    }
                }

                if !constraints.is_empty() {
                    return format!("({})", constraints.join(" && "));
                }
            }
            SchemaType::Array(array_type) => {
                let mut constraints = Vec::new();

                // Length constraints
                if let Some(min_len) = &array_type.min_length {
                    if let Some(max_len) = &array_type.max_length {
                        constraints
                            .push(format!("this.length.isBetween({}, {})", min_len, max_len));
                    } else {
                        constraints.push(format!("this.length >= {}", min_len));
                    }
                } else if let Some(max_len) = &array_type.max_length {
                    constraints.push(format!("this.length <= {}", max_len));
                }

                // Uniqueness constraint
                if let Some(unique) = &array_type.unique {
                    if *unique {
                        constraints.push("this.isDistinct".to_string());
                    }
                }

                // Special length constraints for single element arrays
                if let Some(min_len) = &array_type.min_length {
                    if let Some(max_len) = &array_type.max_length {
                        if *min_len == 1 && *max_len == 1 {
                            constraints.clear(); // Replace length constraint
                            constraints.push("this.single".to_string());
                        }
                    }
                }

                // Check for singleOrNull (0 or 1 elements)
                if let Some(max_len) = &array_type.max_length {
                    if *max_len == 1 && array_type.min_length.is_none() {
                        constraints.retain(|c| !c.contains("length")); // Remove length constraint
                        let single_constraint = if schema.optional {
                            "this.singleOrNull".to_string()
                        } else {
                            "this.single".to_string()
                        };
                        constraints.push(single_constraint);
                    }
                }

                if !constraints.is_empty() {
                    return format!("({})", constraints.join(" && "));
                }
            }
            SchemaType::Object(obj_type) => {
                let mut constraints = Vec::new();

                // Length constraints (key-value pairs)
                if let Some(min_len) = &obj_type.min_length {
                    if let Some(max_len) = &obj_type.max_length {
                        constraints
                            .push(format!("this.length.isBetween({}, {})", min_len, max_len));
                    } else {
                        constraints.push(format!("this.length >= {}", min_len));
                    }
                } else if let Some(max_len) = &obj_type.max_length {
                    constraints.push(format!("this.length <= {}", max_len));
                }

                // Required keys constraint
                if let Some(required_keys) = &obj_type.required {
                    if !required_keys.is_empty() {
                        let keys_list = required_keys
                            .iter()
                            .map(|k| format!("\"{}\"", k))
                            .collect::<Vec<_>>()
                            .join(", ");
                        constraints.push(format!(
                            "List({}).every((k) -> this.containsKey(k))",
                            keys_list
                        ));
                    }
                }

                if !constraints.is_empty() {
                    return format!("({})", constraints.join(" && "));
                }
            }
            _ => return String::new(),
        }

        String::new()
    }

    fn render_default_value(&self, schema: &Schema) -> String {
        if !self.options.include_defaults {
            return String::new();
        }

        // Check for defaults in the schema's inner types
        match &schema.ty {
            SchemaType::Boolean(bool_type) => {
                if let Some(default) = &bool_type.default {
                    return format!(" = {}", default);
                }
            }
            SchemaType::Integer(int_type) => {
                if let Some(default) = &int_type.default {
                    return format!(" = {}", default);
                }
            }
            SchemaType::Float(float_type) => {
                if let Some(default) = &float_type.default {
                    return format!(" = {}", default);
                }
            }
            SchemaType::String(string_type) => {
                if let Some(default) = &string_type.default {
                    return format!(" = \"{}\"", default);
                }
            }
            SchemaType::Array(array_type) => {
                if array_type.default.is_some() {
                    return " = new Listing {}".to_string();
                }
            }
            SchemaType::Object(obj_type) => {
                if obj_type.default.is_some() {
                    return " = new Mapping {}".to_string();
                }
            }
            SchemaType::Enum(enum_type) => {
                if let Some(default) = &enum_type.default {
                    match default {
                        LiteralValue::String(s) => return format!(" = \"{}\"", s),
                        LiteralValue::Integer(i) => return format!(" = {}", i),
                        LiteralValue::Float(f) => return format!(" = {}", f),
                        LiteralValue::Boolean(b) => return format!(" = {}", b),
                    }
                }
            }
            _ => {}
        }

        String::new()
    }

    fn render_field_type(&mut self, schema: &Schema) -> RenderResult<String> {
        let (base_type, has_default) = match &schema.ty {
            SchemaType::Boolean(_) => ("Boolean".to_string(), false),
            SchemaType::Integer(int_type) => {
                // Check for enum values first
                if let Some(enum_values) = &int_type.enum_values {
                    let variants: Vec<String> = enum_values.iter().map(|v| v.to_string()).collect();
                    let enum_type = variants.join("|");
                    let alias_name = format!("IntegerEnum{}", self.typealiases.len());
                    self.typealiases.insert(alias_name.clone(), enum_type);
                    return Ok(alias_name);
                }

                // Check for special integer types based on min/max
                let type_name =
                    if let (Some(min), Some(max)) = (&int_type.minimum, &int_type.maximum) {
                        match (min, max) {
                            (0, 255) => "UInt8".to_string(),
                            (0, 65535) => "UInt16".to_string(),
                            (0, 4294967295) => "UInt32".to_string(),
                            (-128, 127) => "Int8".to_string(),
                            (-32768, 32767) => "Int16".to_string(),
                            (-2147483648, 2147483647) => "Int32".to_string(),
                            _ => "Int".to_string(),
                        }
                    } else {
                        "Int".to_string()
                    };
                (type_name, int_type.default.is_some())
            }
            SchemaType::Float(float_type) => {
                // Check for enum values first
                if let Some(enum_values) = &float_type.enum_values {
                    let variants: Vec<String> = enum_values.iter().map(|v| v.to_string()).collect();
                    let enum_type = variants.join("|");
                    let alias_name = format!("FloatEnum{}", self.typealiases.len());
                    self.typealiases.insert(alias_name.clone(), enum_type);
                    return Ok(alias_name);
                }

                ("Number".to_string(), float_type.default.is_some())
            }
            SchemaType::String(string_type) => {
                // Check for enum values first
                if let Some(enum_values) = &string_type.enum_values {
                    let variants: Vec<String> =
                        enum_values.iter().map(|v| format!("\"{}\"", v)).collect();
                    let enum_type = variants.join("|");
                    let alias_name = format!("StringEnum{}", self.typealiases.len());
                    self.typealiases.insert(alias_name.clone(), enum_type);
                    return Ok(alias_name);
                }

                // Check for special string formats that could be Duration or DataSize
                let type_name = if let Some(format) = &string_type.format {
                    match format.as_str() {
                        "duration" => {
                            if let Some(duration) = &string_type.duration {
                                format!("Duration<{}>", duration.to_lowercase())
                            } else {
                                "Duration".to_string()
                            }
                        }
                        "data-size" | "datasize" => {
                            if let Some(data_size) = &string_type.data_size {
                                format!("DataSize<{}>", data_size.to_lowercase())
                            } else {
                                "DataSize".to_string()
                            }
                        }
                        _ => "String".to_string(),
                    }
                } else {
                    "String".to_string()
                };
                (type_name, string_type.default.is_some())
            }
            SchemaType::Array(array) => {
                let item_type = self.render_field_type(&array.items_type)?;
                (format!("Listing<{}>", item_type), array.default.is_some())
            }
            SchemaType::Object(obj) => {
                let key_type = self.render_field_type(&obj.key_type)?;
                let value_type = self.render_field_type(&obj.value_type)?;
                (
                    format!("Mapping<{}, {}>", key_type, value_type),
                    obj.default.is_some(),
                )
            }
            SchemaType::Tuple(tuple) => {
                // Pkl doesn't have tuples, use Pair for 2-element or Listing for more
                let type_name = if tuple.items_types.len() == 2 {
                    let first = self.render_field_type(&tuple.items_types[0])?;
                    let second = self.render_field_type(&tuple.items_types[1])?;
                    format!("Pair<{}, {}>", first, second)
                } else {
                  // TODO: Handle this union
                    let item_type = self.render_field_type(&tuple.items_types)?;
                    format!("Listing<{}>", item_type)
                };
                (type_name, false)
            }
            SchemaType::Union(union) => {
                let mut types: Vec<String> = Vec::new();
                let mut default_type_index = None;

                // Check if any variant has a default value
                for (i, variant) in union.variants_types.iter().enumerate() {
                    let variant_type = self.render_field_type(variant)?;
                    let has_default = match &variant.ty {
                        SchemaType::Boolean(b) => b.default.is_some(),
                        SchemaType::Integer(int) => int.default.is_some(),
                        SchemaType::Float(f) => f.default.is_some(),
                        SchemaType::String(s) => s.default.is_some(),
                        SchemaType::Array(a) => a.default.is_some(),
                        SchemaType::Object(o) => o.default.is_some(),
                        _ => false,
                    };

                    if has_default && default_type_index.is_none() {
                        default_type_index = Some(i);
                        types.push(format!("*{}", variant_type));
                    } else {
                        types.push(variant_type);
                    }
                }

                let union_type = types.join("|");

                // If it's a complex union, consider creating a typealias
                let final_type = if union.variants_types.len() > 3 {
                    let alias_name = format!("UnionType{}", self.typealiases.len());
                    self.typealiases
                        .insert(alias_name.clone(), union_type.clone());
                    alias_name
                } else {
                    union_type
                };

                (final_type, default_type_index.is_some())
            }
            SchemaType::Enum(enum_type) => {
                let mut variants: Vec<String> = enum_type
                    .values
                    .iter()
                    .map(|v| match v {
                        LiteralValue::String(s) => format!("\"{}\"", s),
                        LiteralValue::Integer(i) => i.to_string(),
                        LiteralValue::Float(f) => f.to_string(),
                        LiteralValue::Boolean(b) => b.to_string(),
                    })
                    .collect();

                // If there's a default, mark the corresponding type with *
                if let Some(default_val) = &enum_type.default {
                    // Find the index of the default value in the variants
                    let default_index = enum_type.values.iter().position(|v| v == default_val).unwrap_or(0);
                    if default_index < variants.len() {
                        variants[default_index] = format!("*{}", variants[default_index]);
                    }
                }

                let enum_type_str = variants.join("|");

                // Create a typealias for the enum
                let alias_name = if enum_type.name.is_empty() {
                    format!("EnumType{}", self.typealiases.len())
                } else {
                    self.to_pascal_case(&enum_type.name.clone())
                };
                if self.typealiases.contains_key(&alias_name)
                    && enum_type_str == self.typealiases[&alias_name]
                {
                    return Ok(alias_name);
                }
                self.typealiases.insert(alias_name.clone(), enum_type_str);
                (alias_name, enum_type.default.is_some())
            }
            SchemaType::Literal(literal) => {
                let literal_str = match &literal.value {
                    LiteralValue::String(s) => format!("\"{}\"", s),
                    LiteralValue::Integer(i) => i.to_string(),
                    LiteralValue::Float(f) => f.to_string(),
                    LiteralValue::Boolean(b) => b.to_string(),
                };
                (literal_str, false)
            }
            SchemaType::Struct(_) => {
              // TODO: Replace with class implementation
                ("Dynamic".to_string(), false)
            }
            SchemaType::Reference(reference) => (self.to_pascal_case(&reference.name), false),
            SchemaType::Null => ("nothing".to_string(), false),
            SchemaType::Unknown => ("unknown".to_string(), false),
        };

        let constraints = self.render_constraints(schema);
        Ok(format!("{}{}", base_type, constraints))
    }

    fn render_docs(&self, description: Option<&str>) -> String {
        if !self.options.include_docs {
            return String::new();
        }

        if let Some(desc) = description {
            if !desc.is_empty() {
                return format!("{}/// {}\n", self.indent(), desc);
            }
        }

        String::new()
    }

    fn render_deprecation(&self, schema: &Schema, field: Option<&SchemaField>) -> String {
        // Check for deprecation in both Schema and SchemaField
        let deprecated = field
            .and_then(|f| f.deprecated.as_ref())
            .or_else(|| schema.deprecated.as_ref());

        if let Some(deprecated_msg) = deprecated {
            if deprecated_msg.is_empty() {
                return format!("{}@Deprecated\n", self.indent());
            } else {
                // Parse the deprecation message for structured info
                // Common patterns: "since v1.2.0" or "Use newField instead"
                let mut parts = Vec::new();

                // Try to extract "since" information
                if let Some(since_match) = deprecated_msg.strip_prefix("since ") {
                    if let Some(version) = since_match.split_whitespace().next() {
                        parts.push(format!(
                            "since = \"{}\"",
                            version.trim_matches(&['v', 'V'][..])
                        ));
                    }
                }

                // Use the full message as the message field
                parts.push(format!("message = \"{}\"", deprecated_msg));

                if parts.len() == 1 {
                    return format!("{}@Deprecated {{ {} }}\n", self.indent(), parts[0]);
                } else {
                    return format!("{}@Deprecated {{ {} }}\n", self.indent(), parts.join("; "));
                }
            }
        }

        String::new()
    }

    /// Convert to camelCase for properties
    fn to_camel_case(&self, name: &str) -> String {
        if name.is_empty() {
            return name.to_string();
        }

        let mut result = String::new();
        let mut capitalize_next = false;
        let mut first_char = true;

        for ch in name.chars() {
            if ch == '_' || ch == '-' {
                capitalize_next = true;
            } else if capitalize_next {
                result.push(ch.to_uppercase().next().unwrap_or(ch));
                capitalize_next = false;
            } else if first_char {
                result.push(ch.to_lowercase().next().unwrap_or(ch));
                first_char = false;
            } else {
                result.push(ch);
            }
        }

        result
    }

    fn render_struct_as_module(
        &mut self,
        name: &str,
        structure: &StructType,
        schema: &Schema,
    ) -> RenderResult<String> {
        let mut output = Vec::new();
        let module_name = self.to_pascal_case(name);

        // Add module documentation
        if let Some(description) = &schema.description {
            output.push(self.render_docs(Some(description)));
        }

        // Start module definition
        output.push(format!("module {}", self.escape_name(&module_name)));
        output.push(String::new()); // Empty line after module declaration

        // Render fields as module properties
        for (field_name, field) in &structure.fields {
            // Skip hidden fields
            if field.hidden {
                continue;
            }

            // Add deprecation annotation first
            output.push(self.render_deprecation(&field.schema, Some(field)));

            // Field documentation (use comment from SchemaField, fallback to schema description)
            let field_description = field.comment.as_ref().or(field.schema.description.as_ref());
            if let Some(description) = field_description {
                output.push(self.render_docs(Some(description)));
            }

            // Determine if field should be hidden
            let hidden_modifier = if field.hidden { "hidden " } else { "" };

            // Field type declaration
            let field_type = self.render_field_type(&field.schema)?;
            let field_name_camel = self.to_camel_case(field_name);
            let escaped_name = self.escape_name(&field_name_camel);
            let optional_marker = if field.optional { "?" } else { "" };
            let default_value = self.render_default_value(&field.schema);

            output.push(format!(
                "{}{}: {}{}{}",
                hidden_modifier, escaped_name, field_type, optional_marker, default_value
            ));
            output.push(String::new()); // Empty line between properties
        }

        Ok(output.join("\n"))
    }

    fn render_as_class(
        &mut self,
        name: &str,
        structure: &StructType,
        schema: &Schema,
    ) -> RenderResult<String> {
        let mut output = Vec::new();
        let class_name = self.to_pascal_case(name);

        // Add class documentation
        if let Some(description) = &schema.description {
            output.push(self.render_docs(Some(description)));
        }

        // Start class definition
        output.push(format!("class {}", self.escape_name(&class_name)));
        output.push(String::new()); // Empty line after class declaration

        // Render fields as class properties
        self.depth += 1;
        for (field_name, field) in &structure.fields {
            // Skip hidden fields
            if field.hidden {
                continue;
            }

            // Add deprecation annotation first
            output.push(self.render_deprecation(&field.schema, Some(field)));

            // Field documentation
            let field_description = field.comment.as_ref().or(field.schema.description.as_ref());
            if let Some(description) = field_description {
                output.push(self.render_docs(Some(description)));
            }

            // Determine if field should be hidden
            let hidden_modifier = if field.hidden { "hidden " } else { "" };

            // Field type declaration
            let field_type = self.render_field_type(&field.schema)?;
            let field_name_camel = self.to_camel_case(field_name);
            let escaped_name = self.escape_name(&field_name_camel);
            let optional_marker = if field.optional { "?" } else { "" };
            let default_value = self.render_default_value(&field.schema);

            output.push(format!(
                "{}{}{}: {}{}{}",
                self.indent(), hidden_modifier, escaped_name, field_type, optional_marker, default_value
            ));
            output.push(String::new()); // Empty line between properties
        }
        self.depth -= 1;

        Ok(output.join("\n"))
    }

    fn render_typealiases(&self) -> String {
        if self.typealiases.is_empty() {
            return String::new();
        }

        let mut output = Vec::new();

        for (alias_name, alias_type) in &self.typealiases {
            output.push(format!("typealias {} = {}", alias_name, alias_type));
        }

        output.push(String::new()); // Empty line after typealiases
        output.join("\n")
    }
}

impl SchemaRenderer<String> for PklSchemaRenderer {
    fn is_reference(&self, name: &str) -> bool {
        self.schemas.contains_key(name)
    }

    fn render_array(&mut self, _array: &ArrayType, _schema: &Schema) -> RenderResult<String> {
        // Arrays are handled in render_field_type
        Ok("Listing<unknown>".to_string())
    }

    fn render_boolean(&mut self, _boolean: &BooleanType, _schema: &Schema) -> RenderResult<String> {
        Ok("Boolean".to_string())
    }

    fn render_enum(&mut self, enum_type: &EnumType, _schema: &Schema) -> RenderResult<String> {
        let variants: Vec<String> = enum_type
            .values
            .iter()
            .map(|v| match v {
                LiteralValue::String(s) => format!("\"{}\"", s),
                LiteralValue::Integer(i) => i.to_string(),
                LiteralValue::Float(f) => f.to_string(),
                LiteralValue::Boolean(b) => b.to_string(),
            })
            .collect();
        Ok(variants.join("|"))
    }

    fn render_float(&mut self, _float: &FloatType, _schema: &Schema) -> RenderResult<String> {
        Ok("Number".to_string())
    }

    fn render_integer(&mut self, _integer: &IntegerType, _schema: &Schema) -> RenderResult<String> {
        Ok("Int".to_string())
    }

    fn render_literal(&mut self, literal: &LiteralType, _schema: &Schema) -> RenderResult<String> {
        match &literal.value {
            LiteralValue::String(s) => Ok(format!("\"{}\"", s)),
            LiteralValue::Integer(i) => Ok(i.to_string()),
            LiteralValue::Float(f) => Ok(f.to_string()),
            LiteralValue::Boolean(b) => Ok(b.to_string()),
        }
    }

    fn render_null(&mut self, _schema: &Schema) -> RenderResult<String> {
        Ok("nothing".to_string())
    }

    fn render_object(&mut self, _object: &ObjectType, _schema: &Schema) -> RenderResult<String> {
        // Objects are handled in render_field_type
        Ok("Mapping<String, unknown>".to_string())
    }

    fn render_reference(&mut self, reference: &str, _schema: &Schema) -> RenderResult<String> {
        Ok(self.to_pascal_case(reference))
    }

    fn render_string(&mut self, _string: &StringType, _schema: &Schema) -> RenderResult<String> {
        Ok("String".to_string())
    }

    fn render_struct(&mut self, structure: &StructType, schema: &Schema) -> RenderResult<String> {
        // For inline structs, render as anonymous type (simplified)
        let mut fields = Vec::new();
        for (field_name, field) in &structure.fields {
            let field_type = self.render_field_type(&field.schema)?;
            let field_name_camel = self.to_camel_case(field_name);
            let escaped_name = self.escape_name(&field_name_camel);
            let optional_marker = if field.optional { "?" } else { "" };
            fields.push(format!(
                "{}: {}{}",
                escaped_name, field_type, optional_marker
            ));
        }

        Ok(format!("{{{}}}", fields.join(", ")))
    }

    fn render_tuple(&mut self, tuple: &TupleType, _schema: &Schema) -> RenderResult<String> {
        if tuple.items_types.len() == 2 {
            let first = self.render_field_type(&tuple.items_types[0])?;
            let second = self.render_field_type(&tuple.items_types[1])?;
            Ok(format!("Pair<{}, {}>", first, second))
        } else if tuple.items_types.len() == 1 {
            let item_type = self.render_field_type(&tuple.items_types[0])?;
            Ok(format!("Listing<{}>", item_type))
        } else if tuple.items_types.len() > 2 {
            // For more than 2 items, treat as dynamic
            return Err(RenderError::UnsupportedSchemaType(
                "Tuples with more than 2 items are not supported in Pkl".to_string(),
            ));
        } else {
            Ok("Dynamic".to_string())
        }
    }

    fn render_union(&mut self, union: &UnionType, _schema: &Schema) -> RenderResult<String> {
        let types: Result<Vec<_>, _> = union
            .variants_types
            .iter()
            .map(|t| self.render_field_type(t))
            .collect();
        Ok(types?.join("|"))
    }

    fn render_unknown(&mut self, _schema: &Schema) -> RenderResult<String> {
        Ok("unknown".to_string())
    }

    fn find_root_schema(&mut self, schemas: &IndexMap<String, Schema>) -> Option<(&String, &Schema)> {
       //
    }

    fn render(&mut self, schemas: IndexMap<String, Schema>) -> RenderResult {
        self.schemas = schemas.clone();

        let mut output = Vec::new();

        // Find the root schema and render as module
        let root_name = self
            .options
            .module_name
            .as_deref()
            .or_else(|| schemas.keys().next().map(|s| s.as_str()))
            .unwrap_or("Config");

        if let Some((_, root_schema)) = schemas.iter().next() {
            match &root_schema.ty {
                SchemaType::Struct(structure) => {
                    output.push(self.render_struct_as_module(root_name, structure, root_schema)?);
                }
                _ => {
                    // For non-struct roots, create a simple module with a single property
                    let module_name = self.to_pascal_case(root_name);
                    output.push(format!("module {}", self.escape_name(&module_name)));
                    output.push(String::new());
                    output.push(format!("value: {}", self.render_field_type(root_schema)?));
                }
            }
        }

        // Render nested classes
        for (name, schema) in schemas.iter().skip(1) {
            if let SchemaType::Struct(structure) = &schema.ty {
                output.push(self.render_as_class(name, structure, schema)?);
            }
        }

        // Add typealiases at the beginning (after module but before classes)
        let typealiases = self.render_typealiases();
        if !typealiases.is_empty() {
            // Insert typealiases after the module declaration
            let module_end = output
                .iter()
                .position(|line| line.trim().is_empty())
                .unwrap_or(1);
            output.insert(module_end + 1, typealiases);
        }

        Ok(output.join("\n"))
    }
}
