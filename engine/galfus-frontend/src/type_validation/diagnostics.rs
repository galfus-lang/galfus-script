use super::DeclarationTypeChecker;
use crate::TypeDiagnosticCode;
use galfus_core::{Diagnostic, NodeId, TypeId};

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn report_type_mismatch(
        &mut self,
        expression: NodeId,
        expected: TypeId,
        actual: TypeId,
    ) {
        let span = self
            .graph
            .syntax()
            .node(expression)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let expected = self.describe_type_for_diagnostic(expected);
        let actual = self.describe_type_for_diagnostic(actual);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::TypeMismatch,
            format!("expected `{expected}`, got `{actual}`"),
            span,
        ));
    }

    pub(super) fn report_integer_literal_out_of_range(
        &mut self,
        literal: NodeId,
        literal_text: &str,
        expected: TypeId,
    ) {
        let span = self
            .graph
            .syntax()
            .node(literal)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let expected = self.describe_type_for_diagnostic(expected);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::TypeMismatch,
            format!("integer literal `{literal_text}` does not fit `{expected}`"),
            span,
        ));
    }

    pub(super) fn report_not_callable(&mut self, target: NodeId, target_type: TypeId) {
        let span = self
            .graph
            .syntax()
            .node(target)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let actual = self.layer.table().describe(target_type);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::NotCallable,
            format!("type `{actual}` is not callable"),
            span,
        ));
    }

    pub(super) fn report_unsupported_operator(&mut self, operator: NodeId, operator_text: &str) {
        let span = self
            .graph
            .syntax()
            .node(operator)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::UnsupportedOperator,
            format!("unsupported operator `{operator_text}`"),
            span,
        ));
    }

    pub(super) fn report_operator_type_error(
        &mut self,
        operator: NodeId,
        expected: &str,
        left: TypeId,
        right: TypeId,
    ) {
        let span = self
            .graph
            .syntax()
            .node(operator)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let left = self.layer.table().describe(left);
        let right = self.layer.table().describe(right);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::UnsupportedOperator,
            format!("operator requires {expected}, got `{left}` and `{right}`"),
            span,
        ));
    }

    pub(super) fn report_unary_operator_type_error(
        &mut self,
        operator: NodeId,
        expected: &str,
        operand: TypeId,
    ) {
        let span = self
            .graph
            .syntax()
            .node(operator)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let operand = self.layer.table().describe(operand);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::UnsupportedOperator,
            format!("operator requires {expected}, got `{operand}`"),
            span,
        ));
    }

    pub(super) fn report_invalid_copy_target(&mut self, expression: NodeId, ty: TypeId) {
        let span = self
            .graph
            .syntax()
            .node(expression)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let ty = self.describe_type_for_diagnostic(ty);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::InvalidCopyTarget,
            format!("cannot copy `{ty}` because fieldless structs are not copyable"),
            span,
        ));
    }

    pub(super) fn report_assignment_to_immutable(&mut self, target: NodeId, name: &str) {
        let span = self
            .graph
            .syntax()
            .node(target)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::AssignmentToImmutable,
            format!("cannot assign to immutable binding `{name}`"),
            span,
        ));
    }

    pub(super) fn report_unknown_member(
        &mut self,
        member: NodeId,
        member_name: &str,
        target_type: TypeId,
    ) {
        let span = self
            .graph
            .syntax()
            .node(member)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let target_type = self.layer.table().describe(target_type);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::UnknownMember,
            format!("type `{target_type}` has no member `{member_name}`"),
            span,
        ));
    }

    pub(super) fn report_invalid_index_target(&mut self, target: NodeId, target_type: TypeId) {
        let span = self
            .graph
            .syntax()
            .node(target)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let target_type = self.layer.table().describe(target_type);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::InvalidIndexTarget,
            format!("type `{target_type}` cannot be indexed"),
            span,
        ));
    }

    pub(super) fn report_invalid_index_type(&mut self, index: NodeId, index_type: TypeId) {
        let span = self
            .graph
            .syntax()
            .node(index)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let index_type = self.layer.table().describe(index_type);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::InvalidIndexType,
            format!("index must be an integer, got `{index_type}`"),
            span,
        ));
    }

    pub(super) fn report_invalid_spread_target(&mut self, spread: NodeId, spread_type: TypeId) {
        let span = self
            .graph
            .syntax()
            .node(spread)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let spread_type = self.layer.table().describe(spread_type);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::InvalidSpreadTarget,
            format!("spread target must be an array, got `{spread_type}`"),
            span,
        ));
    }

    pub(super) fn report_invalid_enum_base_type(&mut self, base: NodeId, base_type: TypeId) {
        let span = self
            .graph
            .syntax()
            .node(base)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let base_type = self.layer.table().describe(base_type);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::InvalidEnumBaseType,
            format!("enum base type must be an integer, got `{base_type}`"),
            span,
        ));
    }

    pub(super) fn report_missing_return(&mut self, function: NodeId, expected: TypeId) {
        let span = self
            .graph
            .syntax()
            .node(function)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let expected = self.describe_type_for_diagnostic(expected);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::MissingReturn,
            format!("function must return `{expected}` on every path"),
            span,
        ));
    }

    pub(super) fn report_return_outside_function(&mut self, return_statement: NodeId) {
        let span = self
            .graph
            .syntax()
            .node(return_statement)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::ReturnOutsideFunction,
            "`return` can only be used inside a function",
            span,
        ));
    }

    pub(super) fn report_initialization_cycle(&mut self, binding: NodeId, name: &str) {
        let span = self
            .graph
            .syntax()
            .node(binding)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::InitializationCycle,
            format!("binding `{name}` participates in an initialization cycle"),
            span,
        ));
    }

    pub(super) fn report_invalid_struct_expansion_target(
        &mut self,
        expansion: NodeId,
        actual: TypeId,
    ) {
        let span = self
            .graph
            .syntax()
            .node(expansion)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let actual = self.describe_type_for_diagnostic(actual);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::InvalidStructExpansionTarget,
            format!("struct expansion target must be a struct, got `{actual}`"),
            span,
        ));
    }

    pub(super) fn report_invalid_struct_spread_target(&mut self, spread: NodeId, actual: TypeId) {
        let span = self
            .graph
            .syntax()
            .node(spread)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let actual = self.describe_type_for_diagnostic(actual);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::InvalidStructSpreadTarget,
            format!("struct literal spread target must be a struct, got `{actual}`"),
            span,
        ));
    }

    pub(super) fn report_cannot_infer_type(&mut self, node: NodeId, message: impl Into<String>) {
        let span = self
            .graph
            .syntax()
            .node(node)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::CannotInferType,
            message.into(),
            span,
        ));
    }

    pub(super) fn report_empty_array_literal(&mut self, array: NodeId) {
        let span = self
            .graph
            .syntax()
            .node(array)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::EmptyArrayLiteral,
            "empty array literal is not allowed",
            span,
        ));
    }

    pub(super) fn report_unknown_struct_field(
        &mut self,
        field: NodeId,
        field_name: &str,
        struct_name: &str,
    ) {
        let span = self
            .graph
            .syntax()
            .node(field)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::UnknownStructField,
            format!("struct `{struct_name}` has no field `{field_name}`"),
            span,
        ));
    }

    pub(super) fn report_duplicate_struct_field(&mut self, field: NodeId, field_name: &str) {
        let span = self
            .graph
            .syntax()
            .node(field)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::DuplicateStructField,
            format!("duplicate field `{field_name}` in struct literal"),
            span,
        ));
    }

    pub(super) fn report_missing_struct_field(
        &mut self,
        literal: NodeId,
        field_name: &str,
        struct_name: &str,
    ) {
        let span = self
            .graph
            .syntax()
            .node(literal)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::MissingStructField,
            format!("missing required field `{field_name}` for struct `{struct_name}`"),
            span,
        ));
    }

    pub(super) fn report_invalid_struct_literal_target(
        &mut self,
        target: NodeId,
        target_name: &str,
    ) {
        let span = self
            .graph
            .syntax()
            .node(target)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::InvalidStructLiteralTarget,
            format!("struct literal target `{target_name}` is not a struct"),
            span,
        ));
    }

    pub(super) fn report_choice_payload_required(&mut self, target: NodeId, variant_name: &str) {
        let span = self
            .graph
            .syntax()
            .node(target)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::ChoicePayloadRequired,
            format!("choice variant `{variant_name}` requires a payload"),
            span,
        ));
    }

    pub(super) fn report_choice_payload_not_allowed(&mut self, target: NodeId, variant_name: &str) {
        let span = self
            .graph
            .syntax()
            .node(target)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::ChoicePayloadNotAllowed,
            format!("choice variant `{variant_name}` does not accept a payload"),
            span,
        ));
    }

    pub(super) fn report_argument_count_mismatch(
        &mut self,
        call: NodeId,
        expected: usize,
        actual: usize,
    ) {
        let span = self
            .graph
            .syntax()
            .node(call)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::ArgumentCountMismatch,
            format!("expected {expected} arguments, got {actual}"),
            span,
        ));
    }

    pub(super) fn report_invalid_condition_type(&mut self, condition: NodeId, actual: TypeId) {
        let span = self
            .graph
            .syntax()
            .node(condition)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let actual = self.layer.table().describe(actual);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::InvalidConditionType,
            format!("condition must be `bool`, got `{actual}`"),
            span,
        ));
    }

    pub(super) fn report_break_outside_loop(&mut self, node: NodeId) {
        let span = self
            .graph
            .syntax()
            .node(node)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::BreakOutsideLoop,
            "`break` can only be used inside a loop",
            span,
        ));
    }

    pub(super) fn report_continue_outside_loop(&mut self, node: NodeId) {
        let span = self
            .graph
            .syntax()
            .node(node)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::ContinueOutsideLoop,
            "`continue` can only be used inside a loop",
            span,
        ));
    }

    pub(super) fn report_invalid_iterable_type(&mut self, iterable: NodeId, actual: TypeId) {
        let span = self
            .graph
            .syntax()
            .node(iterable)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let actual = self.layer.table().describe(actual);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::InvalidIterableType,
            format!("for iterable must satisfy `Iterable`, got `{actual}`"),
            span,
        ));
    }

    pub(super) fn report_invalid_match_pattern_type(
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
            TypeDiagnosticCode::InvalidMatchPatternType,
            format!("match pattern must be compatible with `{expected}`, got `{actual}`"),
            span,
        ));
    }

    pub(super) fn report_incompatible_match_arm_type(
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
            TypeDiagnosticCode::IncompatibleMatchArmType,
            format!("match arm body must be compatible with `{expected}`, got `{actual}`"),
            span,
        ));
    }

    pub(super) fn report_invalid_typeof_arm_type(
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
            TypeDiagnosticCode::InvalidMatchPatternType,
            format!("typeof arm type must be compatible with `{expected}`, got `{actual}`"),
            span,
        ));
    }

    pub(super) fn report_incompatible_typeof_arm_type(
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
            TypeDiagnosticCode::IncompatibleMatchArmType,
            format!("typeof arm body must be compatible with `{expected}`, got `{actual}`"),
            span,
        ));
    }
}
