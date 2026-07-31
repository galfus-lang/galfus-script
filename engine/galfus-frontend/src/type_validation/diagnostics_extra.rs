use super::DeclarationTypeChecker;
use crate::{FunctionType, TypeDiagnosticCode};
use galfus_core::{Diagnostic, NodeId, SymbolId, TypeId};
use std::collections::HashMap;

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn report_invalid_instanceof_pattern_type(
        &mut self,
        pattern: NodeId,
        expected: TypeId,
        actual: TypeId,
    ) {
        let span = self
            .graph
            .syntax()
            .node(pattern)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let expected = self.layer.table().describe(expected);
        let actual = self.layer.table().describe(actual);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::InvalidInstanceofPatternType,
            format!("instanceof pattern must be compatible with `{expected}`, got `{actual}`"),
            span,
        ));
    }

    pub(super) fn report_incompatible_instanceof_arm_type(
        &mut self,
        body: NodeId,
        expected: TypeId,
        actual: TypeId,
    ) {
        let span = self
            .graph
            .syntax()
            .node(body)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let expected = self.layer.table().describe(expected);
        let actual = self.layer.table().describe(actual);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::IncompatibleInstanceofArmType,
            format!("instanceof arm body must be compatible with `{expected}`, got `{actual}`"),
            span,
        ));
    }

    pub(super) fn report_invalid_satisfies_target(&mut self, target: NodeId, target_name: &str) {
        let span = self
            .graph
            .syntax()
            .node(target)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::InvalidSatisfiesTarget,
            format!("satisfies target `{target_name}` is not a constraint"),
            span,
        ));
    }

    pub(super) fn report_missing_constraint_field(
        &mut self,
        item: NodeId,
        struct_name: &str,
        constraint_name: &str,
        field_name: &str,
    ) {
        let span = self
            .graph
            .syntax()
            .node(item)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
        TypeDiagnosticCode::MissingConstraintField,
        format!(
            "struct `{struct_name}` does not satisfy `{constraint_name}`: missing field `{field_name}`"
        ),
        span,
    ));
    }

    pub(super) fn report_constraint_field_type_mismatch(
        &mut self,
        field: NodeId,
        struct_name: &str,
        constraint_name: &str,
        field_name: &str,
        expected: TypeId,
        actual: TypeId,
    ) {
        let span = self
            .graph
            .syntax()
            .node(field)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let expected = self.describe_type_for_diagnostic(expected);
        let actual = self.describe_type_for_diagnostic(actual);

        self.diagnostics.push(Diagnostic::error_with_message(
        TypeDiagnosticCode::ConstraintFieldTypeMismatch,
        format!(
            "struct `{struct_name}` does not satisfy `{constraint_name}`: field `{field_name}` expected `{expected}`, got `{actual}`"
        ),
        span,
    ));
    }

    pub(super) fn report_missing_constraint_function(
        &mut self,
        item: NodeId,
        struct_name: &str,
        constraint_name: &str,
        function_name: &str,
    ) {
        let span = self
            .graph
            .syntax()
            .node(item)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
        TypeDiagnosticCode::MissingConstraintFunction,
        format!(
            "struct `{struct_name}` does not satisfy `{constraint_name}`: missing function `{function_name}`"
        ),
        span,
    ));
    }

    pub(super) fn report_constraint_function_type_mismatch(
        &mut self,
        function: NodeId,
        struct_name: &str,
        constraint_name: &str,
        function_name: &str,
        expected: TypeId,
        actual: TypeId,
    ) {
        let span = self
            .graph
            .syntax()
            .node(function)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let expected = self.describe_type_for_diagnostic(expected);
        let actual = self.describe_type_for_diagnostic(actual);

        self.diagnostics.push(Diagnostic::error_with_message(
        TypeDiagnosticCode::ConstraintFunctionTypeMismatch,
        format!(
            "struct `{struct_name}` does not satisfy `{constraint_name}`: function `{function_name}` expected `{expected}`, got `{actual}`"
        ),
        span,
    ));
    }

    pub(super) fn report_constraint_generic_argument_count_mismatch(
        &mut self,
        target: NodeId,
        constraint_name: &str,
        expected: usize,
        actual: usize,
    ) {
        let span = self
            .graph
            .syntax()
            .node(target)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
        TypeDiagnosticCode::ConstraintGenericArgumentCountMismatch,
        format!(
            "constraint `{constraint_name}` expects {expected} generic argument(s), got {actual}"
        ),
        span,
    ));
    }

    pub(super) fn report_invalid_range_operand_type(
        &mut self,
        operand: NodeId,
        expected: &str,
        actual: TypeId,
    ) {
        let span = self
            .graph
            .syntax()
            .node(operand)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let actual = self.layer.table().describe(actual);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::InvalidRangeOperandType,
            format!("range operand must be {expected}, got `{actual}`"),
            span,
        ));
    }

    pub(super) fn report_generic_argument_count_mismatch(
        &mut self,
        target: NodeId,
        expected: usize,
        actual: usize,
    ) {
        let span = self
            .graph
            .syntax()
            .node(target)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::GenericArgumentCountMismatch,
            format!("expected {expected} generic argument(s), got {actual}"),
            span,
        ));
    }

    pub(super) fn report_generic_argument_bound_mismatch(
        &mut self,
        target: NodeId,
        parameter_name: &str,
        bound: TypeId,
        actual: TypeId,
    ) {
        let span = self
            .graph
            .syntax()
            .node(target)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let bound = self.describe_type_for_diagnostic(bound);
        let actual = self.describe_type_for_diagnostic(actual);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::TypeMismatch,
            format!(
                "generic argument for `{parameter_name}` must satisfy `{bound}`, got `{actual}`"
            ),
            span,
        ));
    }

    pub(super) fn report_call_argument_count_mismatch(
        &mut self,
        call: NodeId,
        function: &FunctionType,
        argument_count: usize,
    ) {
        let parameters = function.parameters();

        let required_count = parameters
            .iter()
            .filter(|parameter| !parameter.has_default() && !parameter.is_rest())
            .count();

        let has_rest = parameters.iter().any(|parameter| parameter.is_rest());

        let max_count = if has_rest {
            None
        } else {
            Some(parameters.len())
        };

        let expected = match max_count {
            Some(max_count) if required_count == max_count => required_count.to_string(),
            Some(max_count) => format!("{required_count}..{max_count}"),
            None => format!("{required_count}+"),
        };

        let span = self
            .graph
            .syntax()
            .node(call)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::ArgumentCountMismatch,
            format!("expected {expected} arguments, got {argument_count}"),
            span,
        ));
    }

    pub(super) fn report_omitted_required_argument(&mut self, argument: NodeId) {
        let span = self
            .graph
            .syntax()
            .node(argument)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::ArgumentCountMismatch,
            "argument cannot be omitted because the matching parameter has no default",
            span,
        ));
    }

    pub(super) fn report_non_exhaustive_match(
        &mut self,
        match_expression: NodeId,
        subject_type: TypeId,
        missing_variants: &[String],
    ) {
        let span = self
            .graph
            .syntax()
            .node(match_expression)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let subject_type = self.describe_type_for_diagnostic(subject_type);
        let missing = missing_variants
            .iter()
            .map(|variant| format!("`{variant}`"))
            .collect::<Vec<_>>()
            .join(", ");

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::NonExhaustiveMatch,
            format!("match over `{subject_type}` is not exhaustive; missing {missing}"),
            span,
        ));
    }

    pub(super) fn report_non_exhaustive_instanceof(
        &mut self,
        instanceof_expression: NodeId,
        subject_type: TypeId,
        missing_types: &[String],
    ) {
        let span = self
            .graph
            .syntax()
            .node(instanceof_expression)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let subject_type = self.describe_type_for_diagnostic(subject_type);
        let missing = missing_types
            .iter()
            .map(|ty| format!("`{ty}`"))
            .collect::<Vec<_>>()
            .join(", ");

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::NonExhaustiveMatch,
            format!("instanceof over `{subject_type}` is not exhaustive; missing {missing}"),
            span,
        ));
    }

    pub(super) fn report_non_exhaustive_typeof(
        &mut self,
        typeof_expression: NodeId,
        subject_type: TypeId,
        missing_types: &[String],
    ) {
        let span = self
            .graph
            .syntax()
            .node(typeof_expression)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let subject_type = self.describe_type_for_diagnostic(subject_type);
        let missing = missing_types
            .iter()
            .map(|ty| format!("`{ty}`"))
            .collect::<Vec<_>>()
            .join(", ");

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::NonExhaustiveMatch,
            format!("typeof over `{subject_type}` is not exhaustive; missing {missing}"),
            span,
        ));
    }

    pub(super) fn report_catch_all_pattern_not_final(&mut self, pattern: NodeId) {
        let span = self
            .graph
            .syntax()
            .node(pattern)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::InvalidPatternOrder,
            "catch-all pattern must be the final arm",
            span,
        ));
    }

    pub(super) fn report_unreachable_pattern(&mut self, pattern: NodeId) {
        let span = self
            .graph
            .syntax()
            .node(pattern)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::warning_with_message(
            TypeDiagnosticCode::UnreachablePattern,
            "pattern is unreachable",
            span,
        ));
    }

    pub(super) fn report_unreachable_code(&mut self, node: NodeId) {
        let span = self
            .graph
            .syntax()
            .node(node)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::warning_with_message(
            TypeDiagnosticCode::UnreachableCode,
            "statement is unreachable",
            span,
        ));
    }

    pub(super) fn report_invalid_decorator_usage(
        &mut self,
        node: NodeId,
        message: impl Into<String>,
    ) {
        let span = self
            .graph
            .syntax()
            .node(node)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::InvalidDecoratorUsage,
            message.into(),
            span,
        ));
    }

    pub(super) fn report_recursive_function_stamp(
        &mut self,
        symbol: SymbolId,
        path: &[SymbolId],
        stamp_functions: &HashMap<SymbolId, NodeId>,
    ) {
        let node = stamp_functions.get(&symbol).copied();
        let span = node
            .and_then(|node| self.graph.syntax().node(node).map(|node| node.span()))
            .unwrap_or_else(|| self.source.span());

        let path = self.describe_stamp_cycle(symbol, path);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::RecursiveFunctionStamp,
            format!("function stamp cannot be recursive through `{path}`"),
            span,
        ));
    }

    fn describe_stamp_cycle(&self, start: SymbolId, path: &[SymbolId]) -> String {
        let mut names = path
            .iter()
            .filter_map(|symbol| self.graph.resolution()?.symbol(*symbol))
            .map(|symbol| symbol.name().to_string())
            .collect::<Vec<_>>();

        if let Some(start_name) = self
            .graph
            .resolution()
            .and_then(|resolution| resolution.symbol(start))
            .map(|symbol| symbol.name().to_string())
        {
            names.push(start_name);
        }

        names.join(" -> ")
    }

    pub(super) fn report_restricted_builtin_symbol(&mut self, identifier: NodeId, name: &str) {
        let span = self
            .graph
            .syntax()
            .node(identifier)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::RestrictedBuiltinSymbol,
            format!("use of restricted builtin name `{name}`"),
            span,
        ));
    }

    pub(super) fn report_invalid_keyword_metadata(
        &mut self,
        node: NodeId,
        message: impl Into<String>,
    ) {
        let span = self
            .graph
            .syntax()
            .node(node)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::InvalidKeywordMetadata,
            message.into(),
            span,
        ));
    }

    pub(super) fn report_duplicate_control_target(&mut self, node: NodeId, name: &str) {
        let span = self
            .graph
            .syntax()
            .node(node)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::DuplicateControlTarget,
            format!("duplicate control target name {name}"),
            span,
        ));
    }

    pub(super) fn report_unresolved_control_target(&mut self, node: NodeId, name: &str) {
        let span = self
            .graph
            .syntax()
            .node(node)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::UnresolvedControlTarget,
            format!("unresolved control target {name}"),
            span,
        ));
    }

    pub(super) fn report_invalid_buffer_element(&mut self, node: NodeId, actual: TypeId) {
        let span = self
            .graph
            .syntax()
            .node(node)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let actual = self.layer.table().describe(actual);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::InvalidBufferElement,
            format!("type `{actual}` is not defaultable or nullable"),
            span,
        ));
    }
}
