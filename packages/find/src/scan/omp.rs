use crate::agent::Agent;

use super::*;

impl Scanner {
    // -----------------------------------------------------------------------
    // omp (oh-my-pi)
    // -----------------------------------------------------------------------

    /// CDXC:PromptSearch 2026-10-17 WHY:
    /// OMP is a Pi fork and writes Pi's session log (`session` header with `id` and `cwd`, then
    /// `message` rows) to `~/.omp/agent/sessions/<project>/<time>_<id>.jsonl`, so Pi's parser reads
    /// it; OMP only adds a padded `title` row at the top. `history.db` holds the same prompts with
    /// less context and is not read. Resume is `omp --session <id>`, as gxserver's resume plan does.
    pub(super) fn scan_omp(&mut self) {
        let base = self.path(".omp/agent/sessions");
        self.scan_pi_session_dir(&base, Agent::Omp);
    }
}
