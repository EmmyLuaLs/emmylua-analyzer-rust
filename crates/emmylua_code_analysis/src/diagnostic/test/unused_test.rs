#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_749() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::Unused,
            r#"
            --- @alias Timer {start: fun(timer, delay: integer, cb: function), stop: fun()}

            --- @return Timer
            local new_timer = function() end


            local timer --- @type Timer?

            local function foo()
                timer = timer or new_timer()

                timer:start(100, function()
                    timer:stop()
                    timer = nil

                    -- code
                end)
            end

            foo()
        "#
        ));
    }
}
