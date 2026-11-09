use crate::parser::blockstate::BlockState;
use std::collections::HashMap;

/// Evaluates simple variable expressions like `thisBlock.name.contains('...')` or `'.../prefix' + var`.
pub fn eval_miex_string(
    expr: &str,
    vars: &HashMap<String, String>,
    blockstate: &BlockState,
) -> String {
    let trimmed = expr.trim();
    if trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2 {
        return trimmed[1..trimmed.len() - 1].to_string();
    }

    // Strip outer parentheses if balanced
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        let mut depth = 0;
        let mut all_enclosed = true;
        for (i, c) in trimmed.char_indices() {
            if c == '(' {
                depth += 1;
            } else if c == ')' {
                depth -= 1;
                if depth == 0 && i < trimmed.len() - 1 {
                    all_enclosed = false;
                    break;
                }
            }
        }
        if all_enclosed {
            return eval_miex_string(&trimmed[1..trimmed.len() - 1], vars, blockstate);
        }
    }

    // Simple ternary check: condition ? val1 : val2
    // We need to find '?' that is not inside quotes or parentheses
    let mut qmark_pos = None;
    let mut depth = 0;
    let mut in_quote = false;
    for (i, c) in trimmed.char_indices() {
        if c == '\'' {
            in_quote = !in_quote;
        } else if !in_quote {
            if c == '(' {
                depth += 1;
            } else if c == ')' {
                depth -= 1;
            } else if c == '?' && depth == 0 {
                qmark_pos = Some(i);
                break;
            }
        }
    }

    if let Some(pos) = qmark_pos {
        let cond_part = &trimmed[..pos];
        let rest = &trimmed[pos + 1..];

        // Find ':' that is not inside quotes or parentheses
        let mut colon_pos = None;
        let mut colon_depth = 0;
        let mut c_in_quote = false;
        for (i, c) in rest.char_indices() {
            if c == '\'' {
                c_in_quote = !c_in_quote;
            } else if !c_in_quote {
                if c == '(' {
                    colon_depth += 1;
                } else if c == ')' {
                    colon_depth -= 1;
                } else if c == ':' && colon_depth == 0 {
                    colon_pos = Some(i);
                    break;
                }
            }
        }

        if let Some(c_pos) = colon_pos {
            let true_val = &rest[..c_pos];
            let false_val = &rest[c_pos + 1..];
            let cond_res = eval_miex_condition(cond_part.trim(), vars, blockstate);
            if cond_res {
                return eval_miex_string(true_val.trim(), vars, blockstate);
            } else {
                return eval_miex_string(false_val.trim(), vars, blockstate);
            }
        }
    }

    // Arithmetic expressions without string literals
    if !trimmed.contains('\'')
        && (trimmed.contains('+')
            || trimmed.contains('-')
            || trimmed.contains('*')
            || trimmed.contains('/'))
    {
        let empty_num_vars = HashMap::new();
        if let Some(val) = eval_miex_arithmetic(trimmed, &empty_num_vars, vars, blockstate) {
            return val.to_string();
        }
    }

    // String concatenation with '+'
    // Check if contains '+' outside quotes/parens
    let mut plus_parts = Vec::new();
    let mut last_idx = 0;
    let mut plus_in_quote = false;
    let mut p_depth = 0;
    for (i, c) in trimmed.char_indices() {
        if c == '\'' {
            plus_in_quote = !plus_in_quote;
        } else if !plus_in_quote {
            if c == '(' {
                p_depth += 1;
            } else if c == ')' {
                p_depth -= 1;
            } else if c == '+' && p_depth == 0 {
                plus_parts.push(&trimmed[last_idx..i]);
                last_idx = i + 1;
            }
        }
    }

    if !plus_parts.is_empty() {
        plus_parts.push(&trimmed[last_idx..]);
        let mut result = String::new();
        for p in plus_parts {
            result.push_str(&eval_miex_string(p.trim(), vars, blockstate));
        }
        return result;
    }

    // Textures.getTextureForPlayerProfile(...)
    if trimmed.starts_with("Textures.getTextureForPlayerProfile(") {
        if let Some(tex) = vars.get("entity_texture").or_else(|| vars.get("texture")) {
            return tex.clone();
        }
        return "minecraft:entity/player/wide/steve".to_string();
    }

    // Direct variable lookup
    if let Some(val) = vars.get(trimmed) {
        return val.clone();
    }

    // thisBlock.name
    if trimmed == "thisBlock.name" {
        return blockstate.name.clone();
    }

    // thisBlock.state.<prop>
    if let Some(prop) = trimmed.strip_prefix("thisBlock.state.") {
        return blockstate
            .properties
            .get(prop.trim())
            .cloned()
            .unwrap_or_else(|| "null".to_string());
    }

    // String method: substring, indexOf, length
    // e.g. thisBlock.name.substring(thisBlock.name.indexOf(':') + 1)
    // or color.substring(0, color.length() - 3)
    if trimmed.contains(".substring(") {
        if let Some((target, sub_arg)) = trimmed.split_once(".substring(") {
            if let Some(arg_str) = sub_arg.strip_suffix(')') {
                let target_eval = eval_miex_string(target.trim(), vars, blockstate);
                let args: Vec<&str> = arg_str.split(',').collect();
                if args.len() == 1 {
                    let mut start_idx = 0usize;
                    let arg0 = args[0].trim();
                    if arg0.contains(".indexOf(':') + 1") {
                        start_idx = target_eval.find(':').map(|i| i + 1).unwrap_or(0);
                    } else if let Ok(s) = arg0.parse::<usize>() {
                        start_idx = s;
                    }
                    if start_idx <= target_eval.len() {
                        return target_eval[start_idx..].to_string();
                    }
                    return String::new();
                } else if args.len() == 2 {
                    let start_eval = eval_miex_string(args[0].trim(), vars, blockstate);
                    let start_idx = start_eval.parse::<usize>().unwrap_or(0);
                    let arg1 = args[1].trim();
                    let end_idx = if arg1.contains(".length() -") {
                        if let Some((_, minus_val)) = arg1.split_once('-') {
                            let m = minus_val.trim().parse::<usize>().unwrap_or(0);
                            target_eval.len().saturating_sub(m)
                        } else {
                            target_eval.len()
                        }
                    } else if let Ok(e) = arg1.parse::<usize>() {
                        e
                    } else {
                        target_eval.len()
                    };
                    let start_clamped = start_idx.min(target_eval.len());
                    let end_clamped = end_idx.clamp(start_clamped, target_eval.len());
                    return target_eval[start_clamped..end_clamped].to_string();
                }
            }
        }
    }

    trimmed.to_string()
}

/// Evaluates condition expressions from MiEx JSON.
pub fn eval_miex_condition(
    cond: &str,
    vars: &HashMap<String, String>,
    blockstate: &BlockState,
) -> bool {
    let mut trimmed = cond.trim();

    // Strip outer parentheses
    while trimmed.starts_with('(') && trimmed.ends_with(')') {
        let mut depth = 0;
        let mut all_enclosed = true;
        for (i, c) in trimmed.char_indices() {
            if c == '(' {
                depth += 1;
            } else if c == ')' {
                depth -= 1;
                if depth == 0 && i < trimmed.len() - 1 {
                    all_enclosed = false;
                    break;
                }
            }
        }
        if all_enclosed {
            trimmed = trimmed[1..trimmed.len() - 1].trim();
        } else {
            break;
        }
    }

    if trimmed == "true" {
        return true;
    }
    if trimmed == "false" || trimmed.is_empty() {
        return false;
    }

    // Logical OR ||
    if trimmed.contains("||") {
        let parts: Vec<&str> = trimmed.split("||").collect();
        return parts
            .iter()
            .any(|p| eval_miex_condition(p.trim(), vars, blockstate));
    }

    // Logical AND &&
    if trimmed.contains("&&") {
        let parts: Vec<&str> = trimmed.split("&&").collect();
        return parts
            .iter()
            .all(|p| eval_miex_condition(p.trim(), vars, blockstate));
    }

    if let Some(rest) = trimmed.strip_prefix('!') {
        return !eval_miex_condition(rest.trim(), vars, blockstate);
    }

    if let Some((left, right)) = trimmed.split_once("==") {
        let l = eval_miex_operand(left.trim(), vars, blockstate);
        let r = eval_miex_operand(right.trim(), vars, blockstate);
        return l == r;
    }

    if let Some((left, right)) = trimmed.split_once("!=") {
        let l = eval_miex_operand(left.trim(), vars, blockstate);
        let r = eval_miex_operand(right.trim(), vars, blockstate);
        return l != r;
    }

    if let Some((left, right)) = trimmed.split_once(">=") {
        let l = eval_miex_string(left.trim(), vars, blockstate)
            .parse::<f32>()
            .unwrap_or(0.0);
        let r = eval_miex_string(right.trim(), vars, blockstate)
            .parse::<f32>()
            .unwrap_or(0.0);
        return l >= r;
    }

    if let Some((left, right)) = trimmed.split_once("<=") {
        let l = eval_miex_string(left.trim(), vars, blockstate)
            .parse::<f32>()
            .unwrap_or(0.0);
        let r = eval_miex_string(right.trim(), vars, blockstate)
            .parse::<f32>()
            .unwrap_or(0.0);
        return l <= r;
    }

    if let Some((left, right)) = trimmed.split_once('>') {
        let l = eval_miex_string(left.trim(), vars, blockstate)
            .parse::<f32>()
            .unwrap_or(0.0);
        let r = eval_miex_string(right.trim(), vars, blockstate)
            .parse::<f32>()
            .unwrap_or(0.0);
        return l > r;
    }

    if let Some((left, right)) = trimmed.split_once('<') {
        let l = eval_miex_string(left.trim(), vars, blockstate)
            .parse::<f32>()
            .unwrap_or(0.0);
        let r = eval_miex_string(right.trim(), vars, blockstate)
            .parse::<f32>()
            .unwrap_or(0.0);
        return l < r;
    }

    if trimmed.contains(".contains(") && trimmed.ends_with(')') {
        if let Some((target, rest)) = trimmed.split_once(".contains(") {
            let target_str = eval_miex_string(target.trim(), vars, blockstate);
            let arg = &rest[..rest.len() - 1].trim();
            let query = arg.trim_matches('\'');
            return target_str.contains(query);
        }
    }

    if trimmed.contains(".endsWith(") && trimmed.ends_with(')') {
        if let Some((target, rest)) = trimmed.split_once(".endsWith(") {
            let target_str = eval_miex_string(target.trim(), vars, blockstate);
            let arg = &rest[..rest.len() - 1].trim();
            let query = arg.trim_matches('\'');
            return target_str.ends_with(query);
        }
    }

    if let Some(val) = vars.get(trimmed) {
        return val == "true" || (!val.is_empty() && val != "false" && val != "0" && val != "null");
    }

    false
}

fn eval_miex_operand(
    operand: &str,
    vars: &HashMap<String, String>,
    blockstate: &BlockState,
) -> String {
    let trimmed = operand.trim();
    if trimmed == "null" {
        return "null".to_string();
    }
    if let Some(prop) = trimmed.strip_prefix("thisBlock.state.") {
        return blockstate
            .properties
            .get(prop)
            .cloned()
            .unwrap_or_else(|| "null".to_string());
    }
    eval_miex_string(trimmed, vars, blockstate)
}

/// Evaluates numerical expressions, e.g. `"-12.0 * scale + offsetX"` or `"22.5 * thisBlock.state.rotation + 180.0"`.
pub fn eval_miex_num(expr: &str, vars: &HashMap<String, f32>) -> f32 {
    let dummy_bs = BlockState::default();
    let empty_str_vars = HashMap::new();
    eval_miex_arithmetic(expr, vars, &empty_str_vars, &dummy_bs).unwrap_or(0.0)
}

/// Robust arithmetic evaluator handling +, -, *, /, parentheses, property lookups, and variables.
pub fn eval_miex_arithmetic(
    expr: &str,
    vars_num: &HashMap<String, f32>,
    vars_str: &HashMap<String, String>,
    blockstate: &BlockState,
) -> Option<f32> {
    let mut trimmed = expr.trim();
    // Strip balanced parentheses
    while trimmed.starts_with('(') && trimmed.ends_with(')') {
        let mut depth = 0;
        let mut all_enclosed = true;
        for (i, c) in trimmed.char_indices() {
            if c == '(' {
                depth += 1;
            } else if c == ')' {
                depth -= 1;
                if depth == 0 && i < trimmed.len() - 1 {
                    all_enclosed = false;
                    break;
                }
            }
        }
        if all_enclosed {
            trimmed = trimmed[1..trimmed.len() - 1].trim();
        } else {
            break;
        }
    }

    if let Ok(v) = trimmed.parse::<f32>() {
        return Some(v);
    }

    if let Some(&v) = vars_num.get(trimmed) {
        return Some(v);
    }
    if let Some(s) = vars_str.get(trimmed) {
        if let Ok(v) = s.parse::<f32>() {
            return Some(v);
        }
    }

    if let Some(prop) = trimmed.strip_prefix("thisBlock.state.") {
        if let Some(val_str) = blockstate.properties.get(prop.trim()) {
            if let Ok(v) = val_str.parse::<f32>() {
                return Some(v);
            }
        }
    }

    // Check addition and subtraction (+ and - outside parens, right to left)
    let mut depth = 0;
    let mut add_sub_pos = None;
    let mut op_char = '+';
    for (i, c) in trimmed.char_indices() {
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
        } else if depth == 0 && (c == '+' || c == '-') && i > 0 {
            let prev = trimmed[..i].trim_end();
            if !prev.is_empty()
                && !prev.ends_with('*')
                && !prev.ends_with('/')
                && !prev.ends_with('+')
                && !prev.ends_with('-')
            {
                add_sub_pos = Some(i);
                op_char = c;
            }
        }
    }

    if let Some(pos) = add_sub_pos {
        let left = &trimmed[..pos];
        let right = &trimmed[pos + 1..];
        let l = eval_miex_arithmetic(left, vars_num, vars_str, blockstate)?;
        let r = eval_miex_arithmetic(right, vars_num, vars_str, blockstate)?;
        return Some(if op_char == '+' { l + r } else { l - r });
    }

    // Check multiplication and division (* and / outside parens)
    let mut mul_div_pos = None;
    let mut md_char = '*';
    let mut depth = 0;
    for (i, c) in trimmed.char_indices() {
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
        } else if depth == 0 && (c == '*' || c == '/') {
            mul_div_pos = Some(i);
            md_char = c;
        }
    }

    if let Some(pos) = mul_div_pos {
        let left = &trimmed[..pos];
        let right = &trimmed[pos + 1..];
        let l = eval_miex_arithmetic(left, vars_num, vars_str, blockstate)?;
        let r = eval_miex_arithmetic(right, vars_num, vars_str, blockstate)?;
        return Some(if md_char == '*' {
            l * r
        } else if r != 0.0 {
            l / r
        } else {
            0.0
        });
    }

    None
}
