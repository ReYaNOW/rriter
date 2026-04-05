// queries.rs
use tree_sitter_html;
pub fn get_params_query(lang_name: &str) -> Option<&'static str> {
    match lang_name {
        "py" => Some(r#"
            ([
                (function_definition parameters: (parameters) @params body: (_) @body)
                (lambda parameters: (lambda_parameters) @params body: (_) @body)
            ])
        "#),
        "rs" => Some(r#"
            ([
                (function_item parameters: (_) @params body: (_) @body)
                (closure_expression parameters: (_) @params body: (_) @body)
            ])
        "#),
        "go" => Some(r#"
            ([
                (function_declaration parameters: (_) @params body: (_) @body)
                (method_declaration parameters: (_) @params body: (_) @body)
                (func_literal parameters: (_) @params body: (_) @body)
            ])
        "#),
        "js" => Some(r#"
            ([
                (function_declaration parameters: (formal_parameters) @params body: (_) @body)
                (method_definition parameters: (formal_parameters) @params body: (_) @body)
                (arrow_function parameters: (_) @params body: (_) @body)
                (function_expression parameters: (formal_parameters) @params body: (_) @body)
            ])
        "#),
        "java" => Some(r#"
            ([
                (method_declaration parameters: (formal_parameters) @params body: (_) @body)
                (constructor_declaration parameters: (formal_parameters) @params body: (_) @body)
            ])
        "#),
        "cs" => Some(r#"
            ([
                (method_declaration (parameter_list) @params body: (_) @body)
                (constructor_declaration (parameter_list) @params body: (_) @body)
                (local_function_statement (parameter_list) @params body: (_) @body)
            ])
        "#),
        "dart" => Some(r#"
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
        "#),
        _ => None,
    }
}
pub fn get_ts_config(lang_name: &str) -> Option<(tree_sitter::Language, Vec<&'static str>)> {
    match lang_name {
        "bash" => Some((
            tree_sitter_bash::LANGUAGE.into(),
            vec![
                "(word) @any_word",
                
                // --- НОВОЕ: Захват флагов команд (начинаются с дефиса) как констант ---
                "((command (_) @constant) (#match? @constant \"^-\"))",
                
                "(function_definition name: (word) @function)",
                "[\"(\" \")\" \"{\" \"}\"] @operator",
                "[\"[\" \"]\"] @keyword.control",
                
                // --- НОВОЕ: Расширенная поддержка строк (в т.ч. Heredoc) ---
                "(string) @string",
                "(raw_string) @string",
                "(heredoc_body) @string",
                "(heredoc_start) @string",
                
                "(comment) @comment",
                "(command_name (word) @command_word)",
                "(command_name) @command_word",
                
                // --- НОВОЕ: Захват всех переменных ---
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
                
                // --- НОВОЕ: Добавлены export, function, select, unset, until ---
                "[\"if\" \"then\" \"elif\" \"else\" \"fi\" \"for\" \"while\" \"do\" \"done\" \"case\" \"esac\" \"in\" \"export\" \"function\" \"select\" \"unset\" \"until\"] @keyword.control",
            ],
        )),
        "rs" => Some((
            tree_sitter_rust::LANGUAGE.into(),
            vec![
                // Базовый захват всех переменных
                "(identifier) @variable",
                
                // --- НОВОЕ: Умное определение типов (PascalCase) и констант (ALL_CAPS) ---
                "((identifier) @constant (#match? @constant \"^[A-Z][A-Z0-9_]+$\"))",
                "((identifier) @type (#match? @type \"^[A-Z]\"))",
                "((scoped_identifier path: (identifier) @type) (#match? @type \"^[A-Z]\"))",
                "((scoped_identifier path: (scoped_identifier name: (identifier) @type)) (#match? @type \"^[A-Z]\"))",
                "((scoped_type_identifier path: (identifier) @type) (#match? @type \"^[A-Z]\"))",
                "((scoped_type_identifier path: (scoped_identifier name: (identifier) @type)) (#match? @type \"^[A-Z]\"))",

                "(string_literal) @string", "(raw_string_literal) @string", "(char_literal) @string",
                "(escape_sequence) @keyword.control", // НОВОЕ: подсветка \n, \t
                
                "(line_comment) @comment", "(block_comment) @comment",
                
                "(function_item name: (identifier) @function)",
                "(function_signature_item name: (identifier) @function)", // НОВОЕ: Сигнатуры в trait
                
                "(call_expression function: (identifier) @function)",
                "(call_expression function: (field_expression field: (field_identifier) @function))",
                "(call_expression function: (scoped_identifier name: (identifier) @function))", // НОВОЕ: пути типа module::func()
                
                // --- НОВОЕ: Захват дженерик-функций ---
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
                "(self) @parameter", // НОВОЕ
                
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

                // --- НОВОЕ: добавлены default, gen, macro_rules!, raw, ref, union, extern ---
                "[\"fn\" \"let\" \"pub\" \"struct\" \"enum\" \"trait\" \"impl\" \"for\" \"while\" \"loop\" \"match\" \"if\" \"else\" \"return\" \"break\" \"continue\" \"type\" \"const\" \"static\" \"use\" \"mod\" \"unsafe\" \"async\" \"await\" \"dyn\" \"where\" \"as\" \"in\" \"move\" \"default\" \"gen\" \"macro_rules!\" \"raw\" \"ref\" \"union\" \"extern\"] @keyword.control",
                
                "[\"=\" \"==\" \"!=\" \"<\" \">\" \"<=\" \">=\" \"+\" \"-\" \"*\" \"/\" \"%\" \"+=\" \"-=\" \"*=\" \"/=\" \"%=\" \"&&\" \"||\" \"!\" \"?\" \"->\" \"=>\" \"&\" \"|\" \":\" \"::\"] @operator",
            ],
        )),
        "py" => Some((
            tree_sitter_python::LANGUAGE.into(),
            vec![
                "(identifier) @py_ident",
                "(attribute attribute: (identifier) @property)",
                
                // --- НОВОЕ: Константы и классы по регистру (как в highlights.scm) ---
                "((identifier) @type (#match? @type \"^[A-Z]\"))",
                "((identifier) @constant (#match? @constant \"^[A-Z][A-Z_]*$\"))",

                "(string) @string",
                "(escape_sequence) @keyword.control", // НОВОЕ: подсветка \n, \t
                "(interpolation) @interpolation",
                "(comment) @comment", "(integer) @number", "(float) @number",
                "(true) @boolean", "(false) @boolean", "(none) @keyword.control",
                
                "[\"def\" \"class\" \"return\" \"pass\" \"continue\" \"break\" \"if\" \"elif\" \"else\" \"for\" \"while\" \"import\" \"from\" \"as\" \"async\" \"await\" \"match\" \"case\" \"try\" \"except\" \"finally\" \"raise\" \"with\" \"global\" \"nonlocal\" \"assert\" \"yield\" \"del\" \"lambda\"] @keyword.control",
                "[\":\" \"=\"] @keyword.control",
                "\"->\" @fg",
                
                // Операторы (сюда добавлены and, or, is, in, not из highlights.scm)
                "[\"==\" \"!=\" \"<\" \">\" \"<=\" \">=\" \"+\" \"-\" \"*\" \"/\" \"//\" \"%\" \"**\" \"+=\" \"-=\" \"*=\" \"/=\" \"//=\" \"%=\" \"**=\" \"&\" \"|\" \"^\" \"~\" \"<<\" \">>\" \"and\" \"or\" \"not\" \"is\" \"in\"] @operator",
                
                "(function_definition name: (identifier) @py_function)",
                "(call function: (identifier) @py_builtin_or_func)",
                "(call function: (attribute attribute: (identifier) @py_function))",
                "(class_definition name: (identifier) @class_name)",
                
                // --- НОВОЕ: Подсветка типов (Type Hints) ---
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
                "(decorator (call function: (identifier) @py_function))", // НОВОЕ: @decorator()
            ],
        )),
        "toml" => Some((
            tree_sitter_toml_ng::LANGUAGE.into(),
            vec![
                "(bare_key) @property",
                
                // --- НОВОЕ: Дополнительные ключи TOML ---
                "(quoted_key) @string",
                "(dotted_key (bare_key) @property)",
                
                "(string) @string",
                "(integer) @number",
                "(float) @number",
                "(boolean) @boolean",
                
                // --- НОВОЕ: Дата и время TOML ---
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
                // Базовый захват переменных
                "(identifier) @variable",
                
                "(type_identifier) @type",
                "(field_identifier) @property",
                "(function_declaration name: (identifier) @function)",
                "(method_declaration name: (field_identifier) @function)",
                
                // --- НОВОЕ: Встроенные функции Go (len, make, append и т.д.) ---
                "((call_expression function: (identifier) @function.builtin) (#match? @function.builtin \"^(append|cap|close|complex|copy|delete|imag|len|make|new|panic|print|println|real|recover)$\"))",
                
                "(call_expression function: (identifier) @function)",
                "(call_expression function: (selector_expression field: (field_identifier) @function))",
                "(parameter_declaration (identifier) @parameter)",
                
                // --- НОВОЕ: Добавлен rune_literal и escape-последовательности ---
                "(interpreted_string_literal) @string", "(raw_string_literal) @string", "(rune_literal) @string",
                "(escape_sequence) @keyword.control",

                "(int_literal) @number", "(float_literal) @number", "(imaginary_literal) @number",
                "(comment) @comment",
                
                // --- НОВОE: Встроенные константы (nil, true, false) ---
                "(true) @boolean",
                "(false) @boolean",
                "(nil) @keyword.control",

                // --- НОВОЕ: Полный список ключевых слов из highlights.scm ---
                "[\"break\" \"case\" \"chan\" \"const\" \"continue\" \"default\" \"defer\" \"else\" \"fallthrough\" \"for\" \"func\" \"go\" \"goto\" \"if\" \"import\" \"interface\" \"map\" \"package\" \"range\" \"return\" \"select\" \"struct\" \"switch\" \"type\" \"var\"] @keyword.control",
                
                // --- НОВОЕ: Полный список операторов из highlights.scm ---
                "[\"=\" \":=\" \"==\" \"!=\" \"<\" \">\" \"<=\" \">=\" \"+\" \"-\" \"*\" \"/\" \"%\" \"&&\" \"||\" \"!\" \"<-\" \"++\" \"--\" \"+=\" \"-=\" \"*=\" \"/=\" \"%=\" \"&\" \"|\" \"^\" \"&=\" \"|=\" \"^=\" \"<<\" \">>\" \"<<=\" \">>=\"] @operator",
            ],
        )),
        "js" => Some((
            tree_sitter_javascript::LANGUAGE.into(),
            vec![
                // --- УЛУЧШЕНО: Базовый захват и специальные идентификаторы ---
                "(identifier) @variable",
                "((identifier) @type (#match? @type \"^[A-Z]\"))", // Классы/Компоненты (PascalCase)
                "((identifier) @constant (#match? @constant \"^[A-Z_][A-Z0-9_]+$\"))", // Константы (SCREAMING_CASE)
                "((identifier) @keyword.control (#match? @keyword.control \"^(arguments|module|console|window|document|require)$\"))",
                "(this) @keyword.control",
                "(super) @keyword.control",
                
                "(property_identifier) @property",
                "(string) @string", "(template_string) @string", "(regex) @string",
                "(number) @number",
                "(comment) @comment",
                
                // --- УЛУЧШЕНО: Все варианты определения функций ---
                "(function_declaration name: (identifier) @function)",
                "(method_definition name: (property_identifier) @function)",
                "(function_expression name: (identifier) @function)",
                "(variable_declarator name: (identifier) @function value: (arrow_function))",
                "(pair key: (property_identifier) @function value: (arrow_function))",
                
                "(call_expression function: (identifier) @function)",
                "(call_expression function: (member_expression property: (property_identifier) @function))",
                
                // --- УЛУЧШЕНО: Деструктуризация и именованные параметры ---
                "(formal_parameters (identifier) @parameter)",
                "(formal_parameters (object_pattern (shorthand_property_identifier_pattern) @parameter))",
                "(formal_parameters (object_pattern (pair_pattern value: (identifier) @parameter)))",
                "(formal_parameters (array_pattern (identifier) @parameter))",

                "(class_declaration name: (identifier) @class_name)",
                
                // --- НОВОЕ: Поддержка JSX (теги, атрибуты, скобки) ---
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
                
                // --- НОВОЕ: Встроенные значения ---
                "(true) @boolean", "(false) @boolean",
                "(null) @keyword.control", "(undefined) @keyword.control",

                // --- НОВОЕ: Полный список ключевых слов ---
                "[\"async\" \"await\" \"break\" \"case\" \"catch\" \"class\" \"const\" \"continue\" \"debugger\" \"default\" \"delete\" \"do\" \"else\" \"export\" \"extends\" \"finally\" \"for\" \"from\" \"function\" \"if\" \"import\" \"in\" \"instanceof\" \"let\" \"new\" \"return\" \"switch\" \"throw\" \"try\" \"typeof\" \"var\" \"void\" \"while\" \"with\" \"yield\"] @keyword.control",
                
                // --- НОВОЕ: Полный список операторов ---
                "[\"=\" \"==\" \"===\" \"!=\" \"!==\" \"<\" \">\" \"<=\" \">=\" \"+\" \"-\" \"*\" \"/\" \"%\" \"**\" \"+=\" \"-=\" \"*=\" \"/=\" \"%=\" \"**=\" \"&&\" \"||\" \"!\" \"?\" \":\" \"=>\" \"&\" \"|\" \"^\" \"~\" \"<<\" \">>\" \">>>\" \"&=\" \"|=\" \"^=\" \"<<=\" \">>=\" \">>>=\" \"??\"] @operator",
            ],
        )),
        "java" => Some((
            tree_sitter_java::LANGUAGE.into(),
            vec![
                // --- НОВОЕ: Константы (SCREAMING_SNAKE_CASE) ---
                "((identifier) @constant (#match? @constant \"^_*[A-Z][A-Z0-9_]+$\"))",

                // --- НОВОЕ: Умное определение типов (PascalCase) ---
                "((field_access object: (identifier) @type) (#match? @type \"^[A-Z]\"))",
                "((scoped_identifier scope: (identifier) @type) (#match? @type \"^[A-Z]\"))",
                "((method_invocation object: (identifier) @type) (#match? @type \"^[A-Z]\"))",

                // Базовый захват (будет переопределен более точными правилами)
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
                
                // --- НОВОЕ: Полный список ключевых слов из highlights.scm ---
                "[\"abstract\" \"assert\" \"break\" \"case\" \"catch\" \"class\" \"continue\" \"default\" \"do\" \"else\" \"enum\" \"exports\" \"extends\" \"final\" \"finally\" \"for\" \"if\" \"implements\" \"import\" \"instanceof\" \"interface\" \"module\" \"native\" \"new\" \"package\" \"private\" \"protected\" \"public\" \"requires\" \"record\" \"return\" \"sealed\" \"static\" \"strictfp\" \"switch\" \"synchronized\" \"throw\" \"throws\" \"transient\" \"try\" \"volatile\" \"while\" \"yield\"] @keyword.control",
                
                "[\"=\" \"==\" \"!=\" \"<\" \">\" \"<=\" \">=\" \"+\" \"-\" \"*\" \"/\" \"%\" \"+=\" \"-=\" \"*=\" \"/=\" \"%=\" \"&&\" \"||\" \"!\" \"?\" \":\" \"++\" \"--\"] @operator",
                "(annotation name: (identifier) @function)",
                "\"@\" @operator",
            ],
        )),
        "cs" => Some((
            tree_sitter_c_sharp::LANGUAGE.into(),
            vec![
                // Базовый захват переменных
                "(identifier) @variable",
                
                // --- НОВОЕ: Умное определение типов (PascalCase) ---
                "((identifier) @type (#match? @type \"^[A-Z]\"))",
                
                "(string_literal) @string", "(interpolated_string_expression) @string", "(character_literal) @string",
                // --- НОВОЕ: Дополнительные виды строк C# ---
                "(raw_string_literal) @string", "(verbatim_string_literal) @string",
                "(interpolation_start) @punctuation.special", "(interpolation_quote) @punctuation.special",
                "(escape_sequence) @keyword.control",

                "(integer_literal) @number", "(real_literal) @number",
                // --- НОВОЕ: Встроенные константы ---
                "(boolean_literal) @boolean", "(null_literal) @keyword.control",
                
                "(comment) @comment",
                
                "(method_declaration name: (identifier) @function)",
                // --- НОВОЕ: Локальные функции C# ---
                "(local_function_statement name: (identifier) @function)",
                
                "(invocation_expression function: (identifier) @function)",
                "(invocation_expression function: (member_access_expression name: (identifier) @function))",
                
                "(class_declaration name: (identifier) @class_name)",
                // --- НОВОЕ: Интерфейсы, enum, record, struct, namespace ---
                "(interface_declaration name: (identifier) @class_name)",
                "(enum_declaration name: (identifier) @class_name)",
                "(struct_declaration (identifier) @class_name)",
                "(record_declaration (identifier) @class_name)",
                "(namespace_declaration name: (identifier) @class_name)",
                "(predefined_type) @type",
                
                "(modifier) @keyword.control",
                "(implicit_type) @keyword.control",
                
                "(parameter name: (identifier) @parameter)",
                
                // --- РАСШИРЕНО: Полный список ключевых слов C# ---
                "[\"class\" \"interface\" \"enum\" \"struct\" \"record\" \"public\" \"private\" \"protected\" \"internal\" \"static\" \"readonly\" \"return\" \"if\" \"else\" \"for\" \"foreach\" \"in\" \"while\" \"do\" \"switch\" \"case\" \"default\" \"break\" \"continue\" \"using\" \"namespace\" \"new\" \"try\" \"catch\" \"finally\" \"throw\" \"this\" \"base\" \"var\" \"out\" \"ref\" \"override\" \"virtual\" \"abstract\" \"sealed\" \"get\" \"set\" \"init\" \"delegate\" \"event\" \"add\" \"alias\" \"as\" \"checked\" \"explicit\" \"extern\" \"global\" \"goto\" \"implicit\" \"lock\" \"notnull\" \"operator\" \"params\" \"remove\" \"sizeof\" \"stackalloc\" \"typeof\" \"unchecked\" \"await\" \"yield\" \"when\" \"from\" \"where\" \"select\" \"with\" \"let\"] @keyword.control",
                
                // --- РАСШИРЕНО: Операторы C# ---
                "[\"=\" \"==\" \"!=\" \"<\" \">\" \"<=\" \">=\" \"+\" \"-\" \"*\" \"/\" \"%\" \"+=\" \"-=\" \"*=\" \"/=\" \"%=\" \"&&\" \"||\" \"!\" \"?\" \":\" \"??\" \"??=\" \"=>\" \"++\" \"--\" \"&\" \"|\" \"^\" \"~\" \"<<\" \">>\" \">>>\" \"<<=\" \">>=\" \">>>=\" \"&=\" \"|=\" \"^=\"] @operator",
            ],
        )),
        "dart" => Some((
            tree_sitter_dart::LANGUAGE.into(),
            vec![
                // 1. Базовый захват переменных (белые)
                "(identifier) @variable",

                // 2. Типы и классы (PascalCase) -> синие (@type)
                "(type_identifier) @type",
                "((identifier) @type (#match? @type \"^[A-Z]\"))",
                "(class_declaration name: (identifier) @type)",
                "(mixin_declaration (identifier) @type)",
                "(extension_declaration name: (identifier) @type)",
                "(extension_type_declaration name: (extension_type_name (identifier) @type))",
                "(enum_declaration name: (identifier) @type)",
                "((type_identifier) @type (#match? @type \"^(int|double|num|String|bool|List|Set|Map|Runes|Symbol|Future|Stream|Iterable|Never|dynamic|Object)$\"))",

                // 3. Ключевые слова (розовые -> @keyword.control)
                "[\"abstract\" \"as\" \"assert\" \"async\" \"async*\" \"augment\" \"await\" \"base\" \"break\" \"case\" \"catch\" \"class\" \"const\" \"continue\" \"covariant\" \"default\" \"deferred\" \"do\" \"else\" \"enum\" \"export\" \"extends\" \"extension\" \"external\" \"factory\" \"final\" \"finally\" \"for\" \"hide\" \"if\" \"implements\" \"import\" \"in\" \"inline\" \"interface\" \"is\" \"late\" \"library\" \"mixin\" \"native\" \"new\" \"on\" \"operator\" \"part\" \"required\" \"return\" \"sealed\" \"show\" \"static\" \"switch\" \"sync*\" \"throw\" \"try\" \"typedef\" \"var\" \"when\" \"while\" \"with\" \"yield\"] @keyword.control",
                "(void_type) @keyword.control", // Явно ловим void (розовый)
                "((identifier) @keyword.control (#match? @keyword.control \"^(this|super)$\"))", // this и super
                
                // 4. Определение функций и методов (зеленые -> @function)
                "(function_signature name: (identifier) @function)",
                "(getter_signature name: (identifier) @function)",
                "(setter_signature name: (identifier) @function)",
                "(constructor_signature name: (identifier) @function)",
                "(constant_constructor_signature (identifier) @function)",
                "(factory_constructor_signature (identifier) @function)",
                "(redirecting_factory_constructor_signature (identifier) @function)",

                // 5. Структурный поиск ВЫЗОВОВ функций и методов (зеленые)
                "((identifier) @function . (selector (argument_part)))",
                "((identifier) @function . (selector (type_arguments)) . (selector (argument_part)))",
                "((selector (unconditional_assignable_selector (identifier) @function)) . (selector (argument_part)))",
                "((selector (unconditional_assignable_selector (identifier) @function)) . (selector (type_arguments)) . (selector (argument_part)))",
                "((selector (conditional_assignable_selector (identifier) @function)) . (selector (argument_part)))",
                "((selector (conditional_assignable_selector (identifier) @function)) . (selector (type_arguments)) . (selector (argument_part)))",

                // 6. Свойства (белые -> @property)
                "(unconditional_assignable_selector (identifier) @property)",
                "(conditional_assignable_selector (identifier) @property)",
                "(initialized_variable_definition name: (identifier) @variable)",
                "(initialized_identifier (identifier) @variable)",
                "(static_final_declaration (identifier) @variable)",

                // Enums
                "(enum_constant name: (identifier) @property)",

                // Параметры
                "(formal_parameter (identifier) @parameter)",
                "(constructor_param (identifier) @parameter)",
                "(super_formal_parameter (identifier) @parameter)",
                "(named_argument (label (identifier) @parameter))",

                // 7. Литералы
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

                // 8. Комментарии
                "(comment) @comment",
                "(block_comment) @comment",
                "(documentation_block_comment) @comment",

                // 9. Аннотации
                "(annotation \"@\" @operator name: (identifier) @function)",

                // 10. Операторы и дженерики
                "(type_arguments \"<\" @operator \">\" @operator)",
                "(type_parameters \"<\" @operator \">\" @operator)",
                "[(relational_operator) (prefix_operator) (negate_operator) (is_operator) (binary_operator)] @operator",
                // Добавлены ":" и "?" в начало этого массива:
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

                // --- Подсветка кастомных переменных CSS, например, --main-color ---
                "((property_name) @variable (#match? @variable \"^--\"))",
                "((plain_value) @variable (#match? @variable \"^--\"))",

                "(pseudo_class_selector (class_name) @function)",
                "(pseudo_element_selector (tag_name) @function)",
                "(attribute_name) @property",
                "(function_name) @function",

                "(string_value) @string",
                "(color_value) @string", // Hex-цвета
                "(attribute_selector (plain_value) @string)",

                "(integer_value) @number",
                "(float_value) @number",
                "(unit) @type",

                "[\"@media\" \"@import\" \"@charset\" \"@namespace\" \"@supports\" \"@keyframes\"] @keyword.control",
                "(at_keyword) @keyword.control",
                
                // --- ИСПРАВЛЕНО: from и to это узлы, а не слова ---
                "(from) @keyword.control",
                "(to) @keyword.control",
                "[\"and\" \"or\" \"not\" \"only\"] @operator",
                
                "(important) @keyword.control",

                "[\"~\" \">\" \"+\" \"-\" \"*\" \"/\" \"=\" \"^=\" \"|=\" \"~=\" \"$=\" \"*=\"] @operator",
                "[\"{\" \")\" \"(\" \"}\"] @operator", // Оставим радужным скобкам
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
                "(erroneous_end_tag_name) @error", // Хотя у нас нет стиля ошибок, захват не помешает
                "[\"<\" \">\" \"</\" \"/>\" \"=\"] @operator",
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
        let languages = ["bash", "rs", "py", "toml", "go", "js", "java", "cs", "dart", "html", "css"];
        let mut all_passed = true;

        for lang_name in languages {
            if let Some((lang, queries)) = get_ts_config(lang_name) {
                // 1. Проверяем запросы для подсветки синтаксиса
                for q_str in queries {
                    if let Err(e) = tree_sitter::Query::new(&lang, q_str) {
                        println!(
                            "❌ Ошибка в языке '{}' (основной запрос):\nЗапрос: {}\nОшибка: {:?}\n",
                            lang_name, q_str, e
                        );
                        all_passed = false;
                    }
                }

                // 2. Проверяем запрос для поиска параметров, если он существует для этого языка
                if let Some(params_q_str) = get_params_query(lang_name) {
                     if let Err(e) = tree_sitter::Query::new(&lang, params_q_str) {
                         println!(
                            "❌ Ошибка в языке '{}' при компиляции запроса параметров:\nОшибка: {:?}",
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