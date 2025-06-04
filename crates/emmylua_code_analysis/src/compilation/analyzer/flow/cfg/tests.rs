use super::*;
use emmylua_parser::LuaParser;

#[cfg(test)]
mod tests {
    // use emmylua_parser::ParserConfig;

    // use crate::compilation::analyzer::flow::cfg::cfg_analyzer::CfgAnalyzer;

    // use super::*;

    // #[test]
    // fn test_simple_cfg() {
    //     let code = r#"
    //     local x = 1
    //     if x > 0 then
    //         print("positive")
    //     else
    //         print("negative")
    //     end
    //     print("done")
    //     "#;

    //     let tree = LuaParser::parse(code, ParserConfig::default());
    //     let chunk = tree.get_chunk_node();
    //     let analyzer = CfgAnalyzer::new();
    //     let cfg = analyzer.analyze_chunk(chunk);

    //     // 验证CFG基本结构
    //     assert!(cfg.entry_block.is_some());
    //     assert!(cfg.exit_block.is_some());
    //     assert!(!cfg.blocks.is_empty());

    //     // 打印CFG用于调试
    //     println!("{}", CfgVisualizer::to_text(&cfg));

    //     let stats = CfgStats::analyze(&cfg);
    //     println!("CFG Stats: {:?}", stats);
    // }
}
