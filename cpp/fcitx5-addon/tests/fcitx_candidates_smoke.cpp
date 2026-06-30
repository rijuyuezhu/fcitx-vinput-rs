#include "vinput_fcitx_bridge/fcitx_candidates.h"

#include <fcitx/candidatelist.h>
#include <fcitx/text.h>

#include <cassert>
#include <vector>

using vinput_fcitx_bridge::BuildResultCandidateList;
using vinput_fcitx_bridge::Candidate;
using vinput_fcitx_bridge::CandidateSource;
using vinput_fcitx_bridge::RecognitionPayload;
using vinput_fcitx_bridge::ResultCandidateComment;

int main() {
  RecognitionPayload empty;
  assert(BuildResultCandidateList(empty) == nullptr);

  assert(ResultCandidateComment({"raw", CandidateSource::Raw}, 0) == "ASR raw");
  assert(ResultCandidateComment({"asr", CandidateSource::Asr}, 0) == "ASR");
  assert(ResultCandidateComment({"llm", CandidateSource::Llm}, 2) == "LLM 2");
  assert(ResultCandidateComment({"cancel", CandidateSource::Cancel}, 0) == "Cancel");

  RecognitionPayload payload;
  payload.commit_text = "polished 2";
  payload.candidates = {
      Candidate{"raw transcript", CandidateSource::Raw},
      Candidate{"polished 1", CandidateSource::Llm},
      Candidate{"polished 2", CandidateSource::Llm},
  };

  std::vector<Candidate> selected_candidates;
  auto candidates = BuildResultCandidateList(
      payload, [&selected_candidates](fcitx::InputContext *input_context,
                                      const Candidate &candidate) {
        assert(input_context == nullptr);
        selected_candidates.push_back(candidate);
      });
  assert(candidates != nullptr);
  assert(candidates->totalSize() == 3);
  assert(candidates->size() == 3);
  assert(candidates->pageSize() == 5);
  assert(candidates->layoutHint() == fcitx::CandidateLayoutHint::Vertical);
  assert(candidates->globalCursorIndex() == 2);
  assert(candidates->candidateFromAll(0).text().toString() == "raw transcript");
#ifdef VINPUT_FCITX5_CORE_HAVE_CANDIDATE_COMMENT
  assert(candidates->candidateFromAll(1).comment().toString() == "LLM 1");
#endif
  assert(candidates->candidateFromAll(2).text().toString() == "polished 2");
#ifdef VINPUT_FCITX5_CORE_HAVE_CANDIDATE_COMMENT
  assert(candidates->candidateFromAll(2).comment().toString() == "LLM 2");
#endif

  candidates->candidateFromAll(1).select(nullptr);
  assert(selected_candidates.size() == 1);
  assert(selected_candidates[0].text == "polished 1");
  assert(selected_candidates[0].source == CandidateSource::Llm);

  return 0;
}
