use tree_sitter_html;

pub fn get_params_query(lang_name: &str) -> Option<&'static str> {
    match lang_name {
        "py" => Some(
            r#"
            ([
                (function_definition parameters: (parameters) @params body: (_) @body)
                (lambda parameters: (lambda_parameters) @params body: (_) @body)
            ])
        "#,
        ),
        "rs" => Some(
            r#"
            ([
                (function_item parameters: (_) @params body: (_) @body)
                (closure_expression parameters: (_) @params body: (_) @body)
            ])
        "#,
        ),
        "go" => Some(
            r#"
            ([
                (function_declaration parameters: (_) @params body: (_) @body)
                (method_declaration parameters: (_) @params body: (_) @body)
                (func_literal parameters: (_) @params body: (_) @body)
            ])
        "#,
        ),
        "js" | "ts" | "tsx" => Some(
            r#"
            ([
                (function_declaration parameters: (formal_parameters) @params body: (_) @body)
                (method_definition parameters: (formal_parameters) @params body: (_) @body)
                (arrow_function parameters: (_) @params body: (_) @body)
                (function_expression parameters: (formal_parameters) @params body: (_) @body)
            ])
        "#,
        ),
        "java" => Some(
            r#"
            ([
                (method_declaration parameters: (formal_parameters) @params body: (_) @body)
                (constructor_declaration parameters: (formal_parameters) @params body: (_) @body)
            ])
        "#,
        ),
        "cs" => Some(
            r#"
            ([
                (method_declaration (parameter_list) @params body: (_) @body)
                (constructor_declaration (parameter_list) @params body: (_) @body)
                (local_function_statement (parameter_list) @params body: (_) @body)
            ])
        "#,
        ),
        "dart" => Some(
            r#"
            ([
                (function_signature) @params
                (method_signature) @params
                (constructor_signature) @params
                (constant_constructor_signature) @params
                (factory_constructor_signature) @params
                (getter_signature) @params
                (setter_signature) @params
                (operator_signature) @params
            ])
        "#,
        ),
        "c" | "cpp" => Some(
            r#"
            ([
                (function_definition declarator: (function_declarator parameters: (parameter_list) @params) body: (_) @body)
            ])
        "#,
        ),
        _ => None,
    }
}

pub fn get_injection_query(lang_name: &str) -> Option<&'static str> {
    match lang_name {
        "html" => Some(
            r#"
            ((script_element (raw_text) @injection.content) (#set! injection.language "js"))
            ((style_element (raw_text) @injection.content) (#set! injection.language "css"))
        "#,
        ),
        "js" | "ts" | "tsx" => Some(
            r#"
            ((regex_pattern) @injection.content (#set! injection.language "regex"))
            (call_expression function: (identifier) @name arguments: (template_string (string_fragment) @injection.content) (#eq? @name "html") (#set! injection.language "html"))
            (call_expression function: (identifier) @name arguments: (template_string (string_fragment) @injection.content) (#eq? @name "css") (#set! injection.language "css"))
        "#,
        ),
        "cpp" => Some(
            r#"
            (raw_string_literal
                delimiter: (raw_string_delimiter) @injection.language
                (raw_string_content) @injection.content)
        "#,
        ),
        "py" => Some(
            r#"
            (call function: (attribute attribute: (identifier) @_name)
                (#match? @_name "^(compile|match|search|sub|subn|split|findall|finditer)$")
                arguments: (argument_list (string) @injection.content)
                (#set! injection.language "regex"))
        "#,
        ),
        "rs" => Some(
            r#"
            (macro_invocation macro: (identifier) @_name (#eq? @_name "html") (token_tree) @injection.content (#set! injection.language "html"))
            (macro_invocation macro: (identifier) @_name (#eq? @_name "regex") (token_tree) @injection.content (#set! injection.language "regex"))
        "#,
        ),
        "make" => Some(
            r#"
            ((shell_text) @injection.content
              (#set! injection.language "bash"))

            ((shell_command) @injection.content
              (#set! injection.language "bash"))
        "#,
        ),
        _ => None,
    }
}

pub fn get_folding_query(lang_name: &str) -> Option<&'static str> {
    match lang_name {
        "rs" => Some(
            r#"
            [
                (function_item) (struct_item) (enum_item) (impl_item) (trait_item) (mod_item)
            ] @sticky
            [
                (macro_definition) (for_expression) (while_expression) (if_expression) (match_expression)
                (loop_expression) (block)
            ] @fold
            [
                (array_expression)
                (macro_invocation)
            ] @autofold
            "#,
        ),
        "py" => Some(
            r#"
            [
                (function_definition) (class_definition)
            ] @sticky
            [
                (if_statement) (for_statement) (while_statement)
                (with_statement) (try_statement) (match_statement) (case_clause)
            ] @fold
            [
                (dictionary)
                (list)
                (tuple)
                (set)
            ] @autofold
            "#,
        ),
        "go" => Some(
            r#"
            [
                (function_declaration) (method_declaration) (type_declaration)
            ] @sticky
            [
                (if_statement) (for_statement) (expression_switch_statement) (type_switch_statement)
            ] @fold
            [
                (literal_value)
                (composite_literal)
            ] @autofold
            "#,
        ),
        "js" => Some(
            r#"
            [
                (function_declaration) (method_definition) (class_declaration)
                (arrow_function) (function_expression)
            ] @sticky
            [
                (if_statement) (for_statement) (while_statement) (switch_statement)
                (try_statement) (statement_block)
            ] @fold
            [
                (object)
                (array)
            ] @autofold
            "#,
        ),
        "ts" | "tsx" => Some(
            r#"
            [
                (function_declaration) (method_definition) (class_declaration) (interface_declaration)
                (arrow_function) (function_expression)
            ] @sticky
            [
                (if_statement) (for_statement) (while_statement) (switch_statement)
                (try_statement) (statement_block)
            ] @fold
            [
                (object)
                (array)
            ] @autofold
            "#,
        ),
        "java" => Some(
            r#"
            [
                (class_declaration) (method_declaration) (constructor_declaration) 
                (interface_declaration) (enum_declaration)
            ] @sticky
            [
                (block)
                (class_body)
                (interface_body)
                (enum_body)
            ] @fold
            [
                (array_initializer)
            ] @autofold
            "#,
        ),
        "cs" => Some(
            r#"
            [
                (class_declaration) (method_declaration) (struct_declaration) (enum_declaration) (interface_declaration) (record_declaration) (namespace_declaration) (constructor_declaration)
            ] @sticky
            [
                (block)
                (switch_body)
            ] @fold
            [
                (initializer_expression)
            ] @autofold
            "#,
        ),
        "dart" => Some(
            r#"
            [
                (class_declaration) (mixin_declaration) (extension_declaration) (enum_declaration)
                (function_signature) (method_signature) (constructor_signature)
            ] @sticky
            [
                (block)
                (class_body)
            ] @fold
            [
                (list_literal)
                (set_or_map_literal)
            ] @autofold
            "#,
        ),
        "c" => Some(
            r#"
            [
                (function_definition) (struct_specifier) (enum_specifier)
            ] @sticky
            [
                (compound_statement)
                (field_declaration_list)
                (enumerator_list)
            ] @fold
            [
                (initializer_list)
            ] @autofold
            "#,
        ),
        "cpp" => Some(
            r#"
            [
                (function_definition) (struct_specifier) (class_specifier) (enum_specifier)
                (namespace_definition)
            ] @sticky
            [
                (compound_statement)
                (declaration_list)
                (field_declaration_list)
                (enumerator_list)
            ] @fold
            [
                (initializer_list)
            ] @autofold
            "#,
        ),
        "css" => Some(
            r#"
            [
                (rule_set)
            ] @sticky
            [
                (block)
            ] @fold
            "#,
        ),
        "json" => Some(
            r#"
            [
                (object)
                (array)
            ] @autofold
            "#,
        ),
        "html" => Some(
            r#"
            (element) @fold
            "#,
        ),
        "make" => Some(
            r#"
            [
              (conditional)
              (rule)
              (define_directive)
            ] @fold
            "#,
        ),
        "bash" => Some(
            r#"
            [
                (function_definition)
            ] @sticky
            [
                (if_statement) (for_statement) (while_statement) (case_statement)
            ] @fold
            "#,
        ),
        _ => None,
    }
}

pub fn get_ts_config(lang_name: &str) -> Option<(tree_sitter::Language, Vec<&'static str>)> {
    match lang_name {
        "bash" => Some((
            tree_sitter_bash::LANGUAGE.into(),
            vec![
                "(word) @any_word",
                "((command (_) @constant) (#match? @constant \"^-\"))",
                "(function_definition name: (word) @function)",
                "[\"(\" \")\" \"{\" \"}\"] @operator",
                "[\"[\" \"]\"] @keyword.control",
                "(string) @string",
                "(raw_string) @string",
                "(heredoc_body) @string",
                "(heredoc_start) @string",
                "(comment) @comment",
                "(command_name (word) @command_word)",
                "(command_name) @command_word",
                "(variable_name) @variable",
                "(variable_assignment name: (variable_name) @variable)",
                "[\"|\" \"||\" \"&&\" \"&\" \">\" \">>\" \"<\" \">&\" \"&>\" \"&>>\" \"<(\" \">(\"] @operator",
                "(process_substitution \")\" @operator)",
                "(file_descriptor) @number",
                "(file_redirect destination: (_) @number)",
                "(expansion \"${\" @subst \"}\" @subst)",
                "(expansion (variable_name) @variable)",
                "(expansion[\":\" \"-\" \"=\" \"+\" \"?\" \":-\"] @fg)",
                "(expansion (_) @fg)",
                "(command_substitution \"$(\" @subst \")\" @subst)",
                "(command_substitution \"`\" @subst)",
                "(simple_expansion \"$\" @subst (variable_name) @variable)",
                "[\"if\" \"then\" \"elif\" \"else\" \"fi\" \"for\" \"while\" \"do\" \"done\" \"case\" \"esac\" \"in\" \"export\" \"function\" \"select\" \"unset\" \"until\"] @keyword.control",
            ],
        )),
        "rs" => Some((
            tree_sitter_rust::LANGUAGE.into(),
            vec![
                "(identifier) @variable",
                "((identifier) @constant (#match? @constant \"^[A-Z][A-Z0-9_]+$\"))",
                "((identifier) @type (#match? @type \"^[A-Z]\"))",
                "((scoped_identifier path: (identifier) @type) (#match? @type \"^[A-Z]\"))",
                "((scoped_identifier path: (scoped_identifier name: (identifier) @type)) (#match? @type \"^[A-Z]\"))",
                "((scoped_type_identifier path: (identifier) @type) (#match? @type \"^[A-Z]\"))",
                "((scoped_type_identifier path: (scoped_identifier name: (identifier) @type)) (#match? @type \"^[A-Z]\"))",
                "(string_literal) @string", "(raw_string_literal) @string", "(char_literal) @string",
                "(escape_sequence) @keyword.control",
                "(line_comment) @comment", "(block_comment) @comment",
                "(function_item name: (identifier) @function)",
                "(function_signature_item name: (identifier) @function)",
                "(call_expression function: (identifier) @function)",
                "(call_expression function: (field_expression field: (field_identifier) @function))",
                "(call_expression function: (scoped_identifier name: (identifier) @function))",
                "(generic_function function: (identifier) @function)",
                "(generic_function function: (scoped_identifier name: (identifier) @function))",
                "(generic_function function: (field_expression field: (field_identifier) @function))",
                "(macro_invocation macro: (identifier) @function)",
                "(macro_invocation macro: (scoped_identifier name: (identifier) @function))",
                "(macro_invocation \"!\" @function)",
                "(type_identifier) @type", "(primitive_type) @type", "(lifetime) @parameter",
                "(integer_literal) @number", "(float_literal) @number",
                "(boolean_literal) @boolean",
                "(parameter (identifier) @parameter)",
                "(closure_parameters (identifier) @parameter)",
                "(self_parameter) @parameter",
                "(self) @parameter",
                "(mutable_specifier) @keyword.control",
                "(crate) @keyword.control",
                "(super) @keyword.control",
                "(use_declaration argument: (scoped_identifier name: (identifier) @type))",
                "(use_declaration argument: (identifier) @type)",
                "(use_list (identifier) @type)",
                "(attribute_item (attribute (identifier) @function))",
                "(attribute (token_tree (identifier) @function))",
                "(attribute_item \"#\" @function)",
                "(attribute_item \"[\" @function)",
                "(attribute_item \"]\" @function)",
                "(inner_attribute_item \"#\" @function)",
                "(inner_attribute_item \"!\" @function)",
                "(inner_attribute_item \"[\" @function)",
                "(inner_attribute_item \"]\" @function)",
                "[\"fn\" \"let\" \"pub\" \"struct\" \"enum\" \"trait\" \"impl\" \"for\" \"while\" \"loop\" \"match\" \"if\" \"else\" \"return\" \"break\" \"continue\" \"type\" \"const\" \"static\" \"use\" \"mod\" \"unsafe\" \"async\" \"await\" \"dyn\" \"where\" \"as\" \"in\" \"move\" \"default\" \"gen\" \"macro_rules!\" \"raw\" \"ref\" \"union\" \"extern\"] @keyword.control",
                "[\"=\" \"==\" \"!=\" \"<\" \">\" \"<=\" \">=\" \"+\" \"-\" \"*\" \"/\" \"%\" \"+=\" \"-=\" \"*=\" \"/=\" \"%=\" \"&&\" \"||\" \"!\" \"?\" \"->\" \"=>\" \"&\" \"|\" \":\" \"::\"] @operator",
            ],
        )),
                "py" => Some((
            tree_sitter_python::LANGUAGE.into(),
            vec![
                "(parameters (list_splat_pattern (identifier) @parameter))",
                "(parameters (dictionary_splat_pattern (identifier) @parameter))",
                "(identifier) @py_ident",
                "(attribute attribute: (identifier) @property)",
                "((identifier) @type (#match? @type \"^_*[A-Z]\"))",
                "((identifier) @type (#match? @type \"^(list|dict|set|tuple|str|int|bool|float|bytes|bytearray|complex|None|Any|Unknown|Sequence|Callable|Generator|AsyncGenerator|Coroutine|Iterable|Mapping|TypeVar|Generic)$\"))",
                "((identifier) @parameter . \":\")",
                "((identifier) @py_function (#match? @py_function \"^__.*__$\"))",
                "((identifier) @constant (#match? @constant \"^[A-Z][A-Z_]*$\"))",
                "(string) @string",
                "(escape_sequence) @keyword.control",
                "(interpolation) @interpolation",
                "(comment) @comment", "(integer) @number", "(float) @number",
                "(true) @boolean", "(false) @boolean", "(none) @keyword.control",
                "[\"def\" \"class\" \"return\" \"pass\" \"continue\" \"break\" \"if\" \"elif\" \"else\" \"for\" \"while\" \"import\" \"from\" \"as\" \"async\" \"await\" \"match\" \"case\" \"try\" \"except\" \"finally\" \"raise\" \"with\" \"global\" \"nonlocal\" \"assert\" \"yield\" \"del\" \"lambda\"] @keyword.control",
                                "[\":\" \"=\"] @keyword.control",
                "\"->\" @operator",
                "[\"==\" \"!=\" \"<\" \">\" \"<=\" \">=\" \"+\" \"-\" \"*\" \"/\" \"//\" \"%\" \"**\" \"+=\" \"-=\" \"*=\" \"/=\" \"//=\" \"%=\" \"**=\" \"&\" \"|\" \"^\" \"~\" \"<<\" \">>\" \"and\" \"or\" \"not\" \"is\" \"in\"] @operator",
                "(function_definition name: (identifier) @py_function)",
                "(call function: (identifier) @py_builtin_or_func)",
                "(call function: (attribute attribute: (identifier) @py_function))",
                "(class_definition name: (identifier) @class_name)",
                "(type (identifier) @type)",
                "(function_definition return_type: (_) @type)",
                "(typed_parameter type: (_) @type)",
                "(typed_default_parameter type: (_) @type)",
                "(parameters (identifier) @parameter)",
                "(parameters (typed_parameter (identifier) @parameter))",
                "(parameters (default_parameter name: (identifier) @parameter))",
                "(parameters (typed_default_parameter name: (identifier) @parameter))",
                "(lambda_parameters (identifier) @parameter)",
                "(keyword_argument name: (identifier) @parameter)",
                "(assignment left: (identifier) @py_assign)",
                "(decorator \"@\" @keyword.control)",
                "(decorator (identifier) @py_function)",
                "(decorator (call function: (identifier) @py_function))",
            ],
        )),
        "toml" => Some((
            tree_sitter_toml_ng::LANGUAGE.into(),
            vec![
                "(bare_key) @property",
                "(quoted_key) @string",
                "(dotted_key (bare_key) @property)",
                "(string) @string",
                "(integer) @number",
                "(float) @number",
                "(boolean) @boolean",
                "(offset_date_time) @constant",
                "(local_date_time) @constant",
                "(local_date) @constant",
                "(local_time) @constant",
                "(comment) @comment",
                "[\"=\" \"[\" \"]\" \"[[\" \"]]\"] @operator",
            ],
        )),
        "go" => Some((
            tree_sitter_go::LANGUAGE.into(),
            vec![
                "(identifier) @variable",
                "(type_identifier) @type",
                "(field_identifier) @property",
                "(function_declaration name: (identifier) @function)",
                "(method_declaration name: (field_identifier) @function)",
                "((call_expression function: (identifier) @function.builtin) (#match? @function.builtin \"^(append|cap|close|complex|copy|delete|imag|len|make|new|panic|print|println|real|recover)$\"))",
                "(call_expression function: (identifier) @function)",
                "(call_expression function: (selector_expression field: (field_identifier) @function))",
                "(parameter_declaration (identifier) @parameter)",
                "(interpreted_string_literal) @string", "(raw_string_literal) @string", "(rune_literal) @string",
                "(escape_sequence) @keyword.control",
                "(int_literal) @number", "(float_literal) @number", "(imaginary_literal) @number",
                "(comment) @comment",
                "(true) @boolean",
                "(false) @boolean",
                "(nil) @keyword.control",
                "[\"break\" \"case\" \"chan\" \"const\" \"continue\" \"default\" \"defer\" \"else\" \"fallthrough\" \"for\" \"func\" \"go\" \"goto\" \"if\" \"import\" \"interface\" \"map\" \"package\" \"range\" \"return\" \"select\" \"struct\" \"switch\" \"type\" \"var\"] @keyword.control",
                "[\"=\" \":=\" \"==\" \"!=\" \"<\" \">\" \"<=\" \">=\" \"+\" \"-\" \"*\" \"/\" \"%\" \"&&\" \"||\" \"!\" \"<-\" \"++\" \"--\" \"+=\" \"-=\" \"*=\" \"/=\" \"%=\" \"&\" \"|\" \"^\" \"&=\" \"|=\" \"^=\" \"<<\" \">>\" \"<<=\" \">>=\"] @operator",
            ],
        )),
        "js" => Some((
            tree_sitter_javascript::LANGUAGE.into(),
            vec![
                "(identifier) @variable",
                "((identifier) @type (#match? @type \"^[A-Z]\"))",
                "((identifier) @constant (#match? @constant \"^[A-Z_][A-Z0-9_]+$\"))",
                "((identifier) @keyword.control (#match? @keyword.control \"^(arguments|module|console|window|document|require)$\"))",
                "(this) @keyword.control",
                "(super) @keyword.control",
                "(property_identifier) @property",
                "(string) @string", "(template_string) @string", "(regex) @string",
                "(number) @number",
                "(comment) @comment",
                "(function_declaration name: (identifier) @function)",
                "(method_definition name: (property_identifier) @function)",
                "(function_expression name: (identifier) @function)",
                "(variable_declarator name: (identifier) @function value: (arrow_function))",
                "(pair key: (property_identifier) @function value: (arrow_function))",
                "(call_expression function: (identifier) @function)",
                "(call_expression function: (member_expression property: (property_identifier) @function))",
                "(formal_parameters (identifier) @parameter)",
                "(formal_parameters (object_pattern (shorthand_property_identifier_pattern) @parameter))",
                "(formal_parameters (object_pattern (pair_pattern value: (identifier) @parameter)))",
                "(formal_parameters (array_pattern (identifier) @parameter))",
                "(class_declaration name: (identifier) @class_name)",
                "((jsx_opening_element (identifier) @keyword.control) (#match? @keyword.control \"^[a-z]\"))",
                "((jsx_closing_element (identifier) @keyword.control) (#match? @keyword.control \"^[a-z]\"))",
                "((jsx_self_closing_element (identifier) @keyword.control) (#match? @keyword.control \"^[a-z]\"))",
                "((jsx_opening_element (identifier) @type) (#match? @type \"^[A-Z]\"))",
                "((jsx_closing_element (identifier) @type) (#match? @type \"^[A-Z]\"))",
                "((jsx_self_closing_element (identifier) @type) (#match? @type \"^[A-Z]\"))",
                "(jsx_opening_element (member_expression) @type)",
                "(jsx_closing_element (member_expression) @type)",
                "(jsx_self_closing_element (member_expression) @type)",
                "(jsx_attribute (property_identifier) @property)",
                "(jsx_opening_element [\"<\" \">\"] @operator)",
                "(jsx_closing_element [\"</\" \">\"] @operator)",
                "(jsx_self_closing_element [\"<\" \"/>\"] @operator)",
                "(true) @boolean", "(false) @boolean",
                "(null) @keyword.control", "(undefined) @keyword.control",
                "[\"async\" \"await\" \"break\" \"case\" \"catch\" \"class\" \"const\" \"continue\" \"debugger\" \"default\" \"delete\" \"do\" \"else\" \"export\" \"extends\" \"finally\" \"for\" \"from\" \"function\" \"if\" \"import\" \"in\" \"instanceof\" \"let\" \"new\" \"return\" \"switch\" \"throw\" \"try\" \"typeof\" \"var\" \"void\" \"while\" \"with\" \"yield\"] @keyword.control",
                "[\"=\" \"==\" \"===\" \"!=\" \"!==\" \"<\" \">\" \"<=\" \">=\" \"+\" \"-\" \"*\" \"/\" \"%\" \"**\" \"+=\" \"-=\" \"*=\" \"/=\" \"%=\" \"**=\" \"&&\" \"||\" \"!\" \"?\" \":\" \"=>\" \"&\" \"|\" \"^\" \"~\" \"<<\" \">>\" \">>>\" \"&=\" \"|=\" \"^=\" \"<<=\" \">>=\" \">>>=\" \"??\"] @operator",
            ],
        )),
        "ts" => Some((
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            vec![
                "(identifier) @variable",
                "((identifier) @type (#match? @type \"^[A-Z]\"))",
                "((identifier) @constant (#match? @constant \"^[A-Z_][A-Z0-9_]+$\"))",
                "((identifier) @keyword.control (#match? @keyword.control \"^(arguments|module|console|window|document|require)$\"))",
                "(this) @keyword.control",
                "(super) @keyword.control",
                "(property_identifier) @property",
                "(string) @string", "(template_string) @string", "(regex) @string",
                "(number) @number",
                "(comment) @comment",
                "(function_declaration name: (identifier) @function)",
                "(method_definition name: (property_identifier) @function)",
                "(function_expression name: (identifier) @function)",
                "(variable_declarator name: (identifier) @function value: (arrow_function))",
                "(pair key: (property_identifier) @function value: (arrow_function))",
                "(call_expression function: (identifier) @function)",
                "(call_expression function: (member_expression property: (property_identifier) @function))",
                "(required_parameter (identifier) @parameter)",
                "(optional_parameter (identifier) @parameter)",
                "(required_parameter (object_pattern (shorthand_property_identifier_pattern) @parameter))",
                "(required_parameter (object_pattern (pair_pattern value: (identifier) @parameter)))",
                "(required_parameter (array_pattern (identifier) @parameter))",
                "(optional_parameter (object_pattern (shorthand_property_identifier_pattern) @parameter))",
                "(optional_parameter (object_pattern (pair_pattern value: (identifier) @parameter)))",
                "(optional_parameter (array_pattern (identifier) @parameter))",
                "(class_declaration name: (type_identifier) @class_name)",
                "(true) @boolean", "(false) @boolean",
                "(null) @keyword.control", "(undefined) @keyword.control",
                "[\"async\" \"await\" \"break\" \"case\" \"catch\" \"class\" \"const\" \"continue\" \"debugger\" \"default\" \"delete\" \"do\" \"else\" \"export\" \"extends\" \"finally\" \"for\" \"from\" \"function\" \"if\" \"import\" \"in\" \"instanceof\" \"let\" \"new\" \"return\" \"switch\" \"throw\" \"try\" \"typeof\" \"var\" \"void\" \"while\" \"with\" \"yield\"] @keyword.control",
                "[\"=\" \"==\" \"===\" \"!=\" \"!==\" \"<\" \">\" \"<=\" \">=\" \"+\" \"-\" \"*\" \"/\" \"%\" \"**\" \"+=\" \"-=\" \"*=\" \"/=\" \"%=\" \"**=\" \"&&\" \"||\" \"!\" \"?\" \":\" \"=>\" \"&\" \"|\" \"^\" \"~\" \"<<\" \">>\" \">>>\" \"&=\" \"|=\" \"^=\" \"<<=\" \">>=\" \">>>=\" \"??\"] @operator",
                "(type_identifier) @type",
                "(predefined_type) @type",
                "[\"abstract\" \"declare\" \"enum\" \"export\" \"implements\" \"interface\" \"keyof\" \"namespace\" \"private\" \"protected\" \"public\" \"type\" \"readonly\" \"override\" \"satisfies\"] @keyword.control",
            ],
        )),
        "tsx" => Some((
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            vec![
                "(identifier) @variable",
                "((identifier) @type (#match? @type \"^[A-Z]\"))",
                "((identifier) @constant (#match? @constant \"^[A-Z_][A-Z0-9_]+$\"))",
                "((identifier) @keyword.control (#match? @keyword.control \"^(arguments|module|console|window|document|require)$\"))",
                "(this) @keyword.control",
                "(super) @keyword.control",
                "(property_identifier) @property",
                "(string) @string", "(template_string) @string", "(regex) @string",
                "(number) @number",
                "(comment) @comment",
                "(function_declaration name: (identifier) @function)",
                "(method_definition name: (property_identifier) @function)",
                "(function_expression name: (identifier) @function)",
                "(variable_declarator name: (identifier) @function value: (arrow_function))",
                "(pair key: (property_identifier) @function value: (arrow_function))",
                "(call_expression function: (identifier) @function)",
                "(call_expression function: (member_expression property: (property_identifier) @function))",
                "(required_parameter (identifier) @parameter)",
                "(optional_parameter (identifier) @parameter)",
                "(required_parameter (object_pattern (shorthand_property_identifier_pattern) @parameter))",
                "(required_parameter (object_pattern (pair_pattern value: (identifier) @parameter)))",
                "(required_parameter (array_pattern (identifier) @parameter))",
                "(optional_parameter (object_pattern (shorthand_property_identifier_pattern) @parameter))",
                "(optional_parameter (object_pattern (pair_pattern value: (identifier) @parameter)))",
                "(optional_parameter (array_pattern (identifier) @parameter))",
                "(class_declaration name: (type_identifier) @class_name)",
                "((jsx_opening_element (identifier) @keyword.control) (#match? @keyword.control \"^[a-z]\"))",
                "((jsx_closing_element (identifier) @keyword.control) (#match? @keyword.control \"^[a-z]\"))",
                "((jsx_self_closing_element (identifier) @keyword.control) (#match? @keyword.control \"^[a-z]\"))",
                "((jsx_opening_element (identifier) @type) (#match? @type \"^[A-Z]\"))",
                "((jsx_closing_element (identifier) @type) (#match? @type \"^[A-Z]\"))",
                "((jsx_self_closing_element (identifier) @type) (#match? @type \"^[A-Z]\"))",
                "(jsx_opening_element (member_expression) @type)",
                "(jsx_closing_element (member_expression) @type)",
                "(jsx_self_closing_element (member_expression) @type)",
                "(jsx_attribute (property_identifier) @property)",
                "(jsx_opening_element [\"<\" \">\"] @operator)",
                "(jsx_closing_element[\"</\" \">\"] @operator)",
                "(jsx_self_closing_element [\"<\" \"/>\"] @operator)",
                "(true) @boolean", "(false) @boolean",
                "(null) @keyword.control", "(undefined) @keyword.control",
                "[\"async\" \"await\" \"break\" \"case\" \"catch\" \"class\" \"const\" \"continue\" \"debugger\" \"default\" \"delete\" \"do\" \"else\" \"export\" \"extends\" \"finally\" \"for\" \"from\" \"function\" \"if\" \"import\" \"in\" \"instanceof\" \"let\" \"new\" \"return\" \"switch\" \"throw\" \"try\" \"typeof\" \"var\" \"void\" \"while\" \"with\" \"yield\"] @keyword.control",
                "[\"=\" \"==\" \"===\" \"!=\" \"!==\" \"<\" \">\" \"<=\" \">=\" \"+\" \"-\" \"*\" \"/\" \"%\" \"**\" \"+=\" \"-=\" \"*=\" \"/=\" \"%=\" \"**=\" \"&&\" \"||\" \"!\" \"?\" \":\" \"=>\" \"&\" \"|\" \"^\" \"~\" \"<<\" \">>\" \">>>\" \"&=\" \"|=\" \"^=\" \"<<=\" \">>=\" \">>>=\" \"??\"] @operator",
                "(type_identifier) @type",
                "(predefined_type) @type",
                "[\"abstract\" \"declare\" \"enum\" \"export\" \"implements\" \"interface\" \"keyof\" \"namespace\" \"private\" \"protected\" \"public\" \"type\" \"readonly\" \"override\" \"satisfies\"] @keyword.control",
            ],
        )),
        "regex" => Some((
            tree_sitter_regex::LANGUAGE.into(),
            vec![
                "[\"(\" \")\" \"(?\" \"(?:\" \"(?<\" \"(?P<\" \"(?P=\" \">\" \"[\" \"]\" \"{\" \"}\" \"[:\" \":]\"] @operator",
                "(group_name) @property",
                "(identity_escape) @keyword.control",
                "(control_letter_escape) @keyword.control",
                "(character_class_escape) @keyword.control",
                "(control_escape) @keyword.control",
                "(start_assertion) @keyword.control",
                "(end_assertion) @keyword.control",
                "(boundary_assertion) @keyword.control",
                "(non_boundary_assertion) @keyword.control",
                "[\"*\" \"+\" \"?\" \"|\" \"=\" \"!\"] @operator",
                "(count_quantifier (decimal_digits) @number)",
                "(count_quantifier \",\" @operator)",
                "(inline_flags_group \"-\" @operator)",
                "(inline_flags_group \":\" @operator)",
                "(flags) @string",
                "(class_character) @string",
                "(posix_class_name) @constant",
                "(pattern_character) @string",
            ],
        )),
        "java" => Some((
            tree_sitter_java::LANGUAGE.into(),
            vec![
                "((identifier) @constant (#match? @constant \"^_*[A-Z][A-Z0-9_]+$\"))",
                "((field_access object: (identifier) @type) (#match? @type \"^[A-Z]\"))",
                "((scoped_identifier scope: (identifier) @type) (#match? @type \"^[A-Z]\"))",
                "((method_invocation object: (identifier) @type) (#match? @type \"^[A-Z]\"))",
                "(identifier) @variable",
                "(type_identifier) @type",
                "(boolean_type) @type", "(integral_type) @type", "(floating_point_type) @type", "(void_type) @type",
                "(string_literal) @string", "(character_literal) @string",
                "(escape_sequence) @keyword.control",
                "(decimal_integer_literal) @number", "(hex_integer_literal) @number", "(octal_integer_literal) @number",
                "(decimal_floating_point_literal) @number", "(hex_floating_point_literal) @number",
                "(true) @boolean", "(false) @boolean", "(null_literal) @keyword.control",
                "(line_comment) @comment", "(block_comment) @comment",
                "(method_declaration name: (identifier) @function)",
                "(method_invocation name: (identifier) @function)",
                "(class_declaration name: (identifier) @class_name)",
                "(interface_declaration name: (identifier) @class_name)",
                "(enum_declaration name: (identifier) @class_name)",
                "(formal_parameter (identifier) @parameter)",
                "(this) @keyword.control",
                "(super) @keyword.control",
                "[\"abstract\" \"assert\" \"break\" \"case\" \"catch\" \"class\" \"continue\" \"default\" \"do\" \"else\" \"enum\" \"exports\" \"extends\" \"final\" \"finally\" \"for\" \"if\" \"implements\" \"import\" \"instanceof\" \"interface\" \"module\" \"native\" \"new\" \"package\" \"private\" \"protected\" \"public\" \"requires\" \"record\" \"return\" \"sealed\" \"static\" \"strictfp\" \"switch\" \"synchronized\" \"throw\" \"throws\" \"transient\" \"try\" \"volatile\" \"while\" \"yield\"] @keyword.control",
                "[\"=\" \"==\" \"!=\" \"<\" \">\" \"<=\" \">=\" \"+\" \"-\" \"*\" \"/\" \"%\" \"+=\" \"-=\" \"*=\" \"/=\" \"%=\" \"&&\" \"||\" \"!\" \"?\" \":\" \"++\" \"--\"] @operator",
                "(annotation name: (identifier) @function)",
                "\"@\" @operator",
            ],
        )),
        "cs" => Some((
            tree_sitter_c_sharp::LANGUAGE.into(),
            vec![
                "(identifier) @variable",
                "((identifier) @type (#match? @type \"^[A-Z]\"))",
                "(string_literal) @string", "(interpolated_string_expression) @string", "(character_literal) @string",
                "(raw_string_literal) @string", "(verbatim_string_literal) @string",
                "(interpolation_start) @punctuation.special", "(interpolation_quote) @punctuation.special",
                "(escape_sequence) @keyword.control",
                "(integer_literal) @number", "(real_literal) @number",
                "(boolean_literal) @boolean", "(null_literal) @keyword.control",
                "(comment) @comment",
                "(method_declaration name: (identifier) @function)",
                "(local_function_statement name: (identifier) @function)",
                "(invocation_expression function: (identifier) @function)",
                "(invocation_expression function: (member_access_expression name: (identifier) @function))",
                "(class_declaration name: (identifier) @class_name)",
                "(interface_declaration name: (identifier) @class_name)",
                "(enum_declaration name: (identifier) @class_name)",
                "(struct_declaration (identifier) @class_name)",
                "(record_declaration (identifier) @class_name)",
                "(namespace_declaration name: (identifier) @class_name)",
                "(predefined_type) @type",
                "(modifier) @keyword.control",
                "(implicit_type) @keyword.control",
                "(parameter name: (identifier) @parameter)",
                "[\"class\" \"interface\" \"enum\" \"struct\" \"record\" \"public\" \"private\" \"protected\" \"internal\" \"static\" \"readonly\" \"return\" \"if\" \"else\" \"for\" \"foreach\" \"in\" \"while\" \"do\" \"switch\" \"case\" \"default\" \"break\" \"continue\" \"using\" \"namespace\" \"new\" \"try\" \"catch\" \"finally\" \"throw\" \"this\" \"base\" \"var\" \"out\" \"ref\" \"override\" \"virtual\" \"abstract\" \"sealed\" \"get\" \"set\" \"init\" \"delegate\" \"event\" \"add\" \"alias\" \"as\" \"checked\" \"explicit\" \"extern\" \"global\" \"goto\" \"implicit\" \"lock\" \"notnull\" \"operator\" \"params\" \"remove\" \"sizeof\" \"stackalloc\" \"typeof\" \"unchecked\" \"await\" \"yield\" \"when\" \"from\" \"where\" \"select\" \"with\" \"let\"] @keyword.control",
                "[\"=\" \"==\" \"!=\" \"<\" \">\" \"<=\" \">=\" \"+\" \"-\" \"*\" \"/\" \"%\" \"+=\" \"-=\" \"*=\" \"/=\" \"%=\" \"&&\" \"||\" \"!\" \"?\" \":\" \"??\" \"??=\" \"=>\" \"++\" \"--\" \"&\" \"|\" \"^\" \"~\" \"<<\" \">>\" \">>>\" \"<<=\" \">>=\" \">>>=\" \"&=\" \"|=\" \"^=\"] @operator",
            ],
        )),
        "dart" => Some((
            tree_sitter_dart::LANGUAGE.into(),
            vec![
                "(identifier) @variable",
                "(type_identifier) @type",
                "((identifier) @type (#match? @type \"^[A-Z]\"))",
                "(class_declaration name: (identifier) @type)",
                "(mixin_declaration (identifier) @type)",
                "(extension_declaration name: (identifier) @type)",
                "(extension_type_declaration name: (extension_type_name (identifier) @type))",
                "(enum_declaration name: (identifier) @type)",
                "((type_identifier) @type (#match? @type \"^(int|double|num|String|bool|List|Set|Map|Runes|Symbol|Future|Stream|Iterable|Never|dynamic|Object)$\"))",
                "[\"abstract\" \"as\" \"assert\" \"async\" \"async*\" \"augment\" \"await\" \"base\" \"break\" \"case\" \"catch\" \"class\" \"const\" \"continue\" \"covariant\" \"default\" \"deferred\" \"do\" \"else\" \"enum\" \"export\" \"extends\" \"extension\" \"external\" \"factory\" \"final\" \"finally\" \"for\" \"hide\" \"if\" \"implements\" \"import\" \"in\" \"inline\" \"interface\" \"is\" \"late\" \"library\" \"mixin\" \"native\" \"new\" \"on\" \"operator\" \"part\" \"required\" \"return\" \"sealed\" \"show\" \"static\" \"switch\" \"sync*\" \"throw\" \"try\" \"typedef\" \"var\" \"when\" \"while\" \"with\" \"yield\"] @keyword.control",
                "(void_type) @keyword.control",
                "((identifier) @keyword.control (#match? @keyword.control \"^(this|super)$\"))",
                "(function_signature name: (identifier) @function)",
                "(getter_signature name: (identifier) @function)",
                "(setter_signature name: (identifier) @function)",
                "(constructor_signature name: (identifier) @function)",
                "(constant_constructor_signature (identifier) @function)",
                "(factory_constructor_signature (identifier) @function)",
                "(redirecting_factory_constructor_signature (identifier) @function)",
                "((identifier) @function . (selector (argument_part)))",
                "((identifier) @function . (selector (type_arguments)) . (selector (argument_part)))",
                "((selector (unconditional_assignable_selector (identifier) @function)) . (selector (argument_part)))",
                "((selector (unconditional_assignable_selector (identifier) @function)) . (selector (type_arguments)) . (selector (argument_part)))",
                "((selector (conditional_assignable_selector (identifier) @function)) . (selector (argument_part)))",
                "((selector (conditional_assignable_selector (identifier) @function)) . (selector (type_arguments)) . (selector (argument_part)))",
                "(unconditional_assignable_selector (identifier) @property)",
                "(conditional_assignable_selector (identifier) @property)",
                "(initialized_variable_definition name: (identifier) @variable)",
                "(initialized_identifier (identifier) @variable)",
                "(static_final_declaration (identifier) @variable)",
                "(enum_constant name: (identifier) @property)",
                "(formal_parameter (identifier) @parameter)",
                "(constructor_param (identifier) @parameter)",
                "(super_formal_parameter (identifier) @parameter)",
                "(named_argument (label (identifier) @parameter))",
                "[(decimal_integer_literal) (hex_integer_literal) (decimal_floating_point_literal)] @number",
                "(string_literal) @string",
                "(template_chars_single) @string",
                "(template_chars_double) @string",
                "(template_chars_single_single) @string",
                "(template_chars_double_single) @string",
                "(template_chars_raw_slash) @string",
                "(true) @boolean",
                "(false) @boolean",
                "(null_literal) @keyword.control",
                "(comment) @comment",
                "(block_comment) @comment",
                "(documentation_block_comment) @comment",
                "(annotation \"@\" @operator name: (identifier) @function)",
                "(type_arguments \"<\" @operator \">\" @operator)",
                "(type_parameters \"<\" @operator \">\" @operator)",
                "[(relational_operator) (prefix_operator) (negate_operator) (is_operator) (binary_operator)] @operator",
                "[\":\" \"?\" \"=\" \"+=\" \"-=\" \"*=\" \"/=\" \"%=\" \"~/=\" \"<<=\" \">>=\" \">>>=\" \"&=\" \"^=\" \"|=\" \"??=\" \"==\" \"!=\" \"+\" \"-\" \"*\" \"/\" \"%\" \"~/\" \"<<\" \">>\" \">>>\" \"&\" \"^\" \"|\" \"&&\" \"||\" \"??\" \"!\" \"~\" \"++\" \"--\" \"..\" \"?..\" \"...\" \"...?\" \"=>\"] @operator",
            ],
        )),
        "css" => Some((
            tree_sitter_css::LANGUAGE.into(),
            vec![
                "(comment) @comment",
                "(tag_name) @keyword.control",
                "(universal_selector) @operator",
                "(class_name) @property",
                "(id_name) @property",
                "(property_name) @property",
                "((property_name) @variable (#match? @variable \"^--\"))",
                "((plain_value) @variable (#match? @variable \"^--\"))",
                "(pseudo_class_selector (class_name) @function)",
                "(pseudo_element_selector (tag_name) @function)",
                "(attribute_name) @property",
                "(function_name) @function",
                "(string_value) @string",
                "(color_value) @string",
                "(attribute_selector (plain_value) @string)",
                "(integer_value) @number",
                "(float_value) @number",
                "(unit) @type",
                "[\"@media\" \"@import\" \"@charset\" \"@namespace\" \"@supports\" \"@keyframes\"] @keyword.control",
                "(at_keyword) @keyword.control",
                "(from) @keyword.control",
                "(to) @keyword.control",
                "[\"and\" \"or\" \"not\" \"only\"] @operator",
                "(important) @keyword.control",
                "[\"~\" \">\" \"+\" \"-\" \"*\" \"/\" \"=\" \"^=\" \"|=\" \"~=\" \"$=\" \"*=\"] @operator",
                "[\"{\" \")\" \"(\" \"}\"] @operator",
                "[\"#\" \",\" \".\" \":\" \"::\" \";\"] @operator",
            ],
        )),
        "html" => Some((
            tree_sitter_html::LANGUAGE.into(),
            vec![
                "(comment) @comment",
                "(tag_name) @keyword.control",
                "(attribute_name) @parameter",
                "(attribute_value) @string",
                "(doctype) @constant",
                "(erroneous_end_tag_name) @error",
                "[\"<\" \">\" \"</\" \"/>\" \"=\"] @operator",
            ],
        )),
        "json" => Some((
            tree_sitter_json::LANGUAGE.into(),
            vec![
                "(pair key: (_) @property)",
                "(string) @string",
                "(number) @number",
                "(null) @keyword.control",
                "(true) @boolean",
                "(false) @boolean",
                "(escape_sequence) @keyword.control",
                "(comment) @comment",
            ],
        )),
        "c" => Some((
            tree_sitter_c::LANGUAGE.into(),
            vec![
                "(identifier) @variable",
                "((identifier) @constant (#match? @constant \"^[A-Z][A-Z\\\\d_]*$\"))",
                "[\"break\" \"case\" \"const\" \"continue\" \"default\" \"do\" \"else\" \"enum\" \"extern\" \"for\" \"if\" \"inline\" \"return\" \"sizeof\" \"static\" \"struct\" \"switch\" \"typedef\" \"union\" \"volatile\" \"while\"] @keyword.control",
                "[\"#define\" \"#elif\" \"#else\" \"#endif\" \"#if\" \"#ifdef\" \"#ifndef\" \"#include\"] @keyword.control",
                "(preproc_directive) @keyword.control",
                "[\"--\" \"-\" \"-=\" \"->\" \"=\" \"!=\" \"*\" \"&\" \"&&\" \"+\" \"++\" \"+=\" \"<\" \"==\" \">\" \"||\"] @operator",
                "(string_literal) @string",
                "(system_lib_string) @string",
                "(null) @keyword.control",
                "(number_literal) @number",
                "(char_literal) @number",
                "(field_identifier) @property",
                "(statement_identifier) @property",
                "(type_identifier) @type",
                "(primitive_type) @type",
                "(sized_type_specifier) @type",
                "(call_expression function: (identifier) @function)",
                "(call_expression function: (field_expression field: (field_identifier) @function))",
                "(function_declarator declarator: (identifier) @function)",
                "(preproc_function_def name: (identifier) @function)",
                "(comment) @comment",
            ],
        )),
        "cpp" => Some((
            tree_sitter_cpp::LANGUAGE.into(),
            vec![
                "(identifier) @variable",
                "((identifier) @constant (#match? @constant \"^[A-Z][A-Z\\\\d_]*$\"))",
                "[\"break\" \"case\" \"const\" \"continue\" \"default\" \"do\" \"else\" \"enum\" \"extern\" \"for\" \"if\" \"inline\" \"return\" \"sizeof\" \"static\" \"struct\" \"switch\" \"typedef\" \"union\" \"volatile\" \"while\" \"catch\" \"class\" \"co_await\" \"co_return\" \"co_yield\" \"constexpr\" \"constinit\" \"consteval\" \"delete\" \"explicit\" \"final\" \"friend\" \"mutable\" \"namespace\" \"noexcept\" \"new\" \"override\" \"private\" \"protected\" \"public\" \"template\" \"throw\" \"try\" \"typename\" \"using\" \"concept\" \"requires\" \"virtual\"] @keyword.control",
                "[\"#define\" \"#elif\" \"#else\" \"#endif\" \"#if\" \"#ifdef\" \"#ifndef\" \"#include\"] @keyword.control",
                "(preproc_directive) @keyword.control",
                "[\"--\" \"-\" \"-=\" \"->\" \"=\" \"!=\" \"*\" \"&\" \"&&\" \"+\" \"++\" \"+=\" \"<\" \"==\" \">\" \"||\"] @operator",
                "(string_literal) @string",
                "(system_lib_string) @string",
                "(raw_string_literal) @string",
                "(null) @keyword.control",
                "(this) @keyword.control",
                "\"nullptr\" @keyword.control",
                "(number_literal) @number",
                "(char_literal) @number",
                "(field_identifier) @property",
                "(statement_identifier) @property",
                "(type_identifier) @type",
                "(primitive_type) @type",
                "(sized_type_specifier) @type",
                "(auto) @type",
                "((namespace_identifier) @type (#match? @type \"^[A-Z]\"))",
                "(call_expression function: (identifier) @function)",
                "(call_expression function: (field_expression field: (field_identifier) @function))",
                "(function_declarator declarator: (identifier) @function)",
                "(preproc_function_def name: (identifier) @function)",
                "(call_expression function: (qualified_identifier name: (identifier) @function))",
                "(template_function name: (identifier) @function)",
                "(template_method name: (field_identifier) @function)",
                "(function_declarator declarator: (qualified_identifier name: (identifier) @function))",
                "(function_declarator declarator: (field_identifier) @function)",
                "(comment) @comment",
            ],
        )),
        "make" => Some((
            tree_sitter_make::LANGUAGE.into(),
            vec![
                "(comment) @comment",
                "((conditional (_) @keyword.control) (#any-of? @keyword.control \"ifeq\" \"else\" \"ifneq\" \"ifdef\" \"ifndef\"))",
                "(conditional \"endif\" @keyword.control)",
                "(rule (targets (word) @function))",
                "((rule (targets) @_target (prerequisites (word) @function)) (#eq? @_target \".PHONY\"))",
                "((rule (targets (word) @function.builtin)) (#match? @function.builtin \"^\\\\.\"))",
                "(rule [\"&:\" \":\" \"::\" \"|\"] @operator)",
                "[\"export\" \"unexport\"] @keyword.import",
                "(override_directive \"override\" @keyword)",
                "(include_directive [\"include\" \"-include\"] @keyword.import)",
                "(include_directive (list (word) @string))",
                "(variable_assignment name: (word) @variable [\"?=\" \":=\" \"::=\" \"+=\" \"=\"] @operator)",
                "(shell_assignment name: (word) @variable \"!=\" @operator)",
                "(define_directive \"define\" @keyword name: (word) @variable \"endef\" @keyword)",
                "((variable_assignment (word) @constant) (#any-of? @constant \".DEFAULT_GOAL\" \".EXTRA_PREREQS\" \".FEATURES\" \".INCLUDE_DIRS\" \".RECIPEPREFIX\" \".SHELLFLAGS\" \".VARIABLES\" \"MAKEARGS\" \"MAKEFILE_LIST\" \"MAKEFLAGS\" \"MAKE_RESTARTS\" \"MAKE_TERMERR\" \"MAKE_TERMOUT\" \"SHELL\"))",
                "(variable_reference (word) @variable)",
                "(shell_function \"shell\" @function)",
                "(automatic_variable) @keyword.control",
                "(recipe_line \"@\" @operator)",
                "\"\\\\\" @operator",
                "((function_call (_) @function.builtin) (#any-of? @function.builtin \"subst\" \"patsubst\" \"strip\" \"findstring\" \"filter\" \"filter-out\" \"sort\" \"word\" \"words\" \"wordlist\" \"firstword\" \"lastword\" \"dir\" \"notdir\" \"suffix\" \"basename\" \"addsuffix\" \"addprefix\" \"join\" \"wildcard\" \"realpath\" \"abspath\" \"error\" \"warning\" \"info\" \"origin\" \"flavor\" \"foreach\" \"if\" \"or\" \"and\" \"call\" \"eval\" \"file\" \"value\"))",
            ],
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_tree_sitter_queries_are_valid() {
        let languages = [
            "bash", "rs", "py", "toml", "go", "js", "ts", "tsx", "regex", "java", "cs", "dart",
            "html", "css", "json", "c", "cpp", "make",
        ];
        let mut all_passed = true;

        for lang_name in languages {
            if let Some((lang, queries)) = get_ts_config(lang_name) {
                for q_str in queries {
                    if let Err(e) = tree_sitter::Query::new(&lang, q_str) {
                        println!(
                            "❌ Ошибка в языке '{}' (основной запрос):\nЗапрос: {}\nОшибка: {:?}\n",
                            lang_name, q_str, e
                        );
                        all_passed = false;
                    }
                }

                if let Some(params_q_str) = get_params_query(lang_name) {
                    if let Err(e) = tree_sitter::Query::new(&lang, params_q_str) {
                        println!(
                            "❌ Ошибка в языке '{}' при компиляции запроса параметров:\nОшибка: {:?}",
                            lang_name, e
                        );
                        all_passed = false;
                    }
                }

                if let Some(inj_q_str) = get_injection_query(lang_name) {
                    if let Err(e) = tree_sitter::Query::new(&lang, inj_q_str) {
                        println!(
                            "❌ Ошибка в языке '{}' при компиляции injection запроса:\nОшибка: {:?}",
                            lang_name, e
                        );
                        all_passed = false;
                    }
                }

                if let Some(fold_q_str) = get_folding_query(lang_name) {
                    if let Err(e) = tree_sitter::Query::new(&lang, fold_q_str) {
                        println!(
                            "❌ Ошибка в языке '{}' при компиляции folding запроса:\nОшибка: {:?}",
                            lang_name, e
                        );
                        all_passed = false;
                    }
                }
            }
        }

        assert!(all_passed, "Есть ошибки в Tree-sitter запросах!");
    }
}
