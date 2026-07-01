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
using vinput_fcitx_bridge::ResultCandidateMenuTitle;

int main() {
  RecognitionPayload empty;
  assert(BuildResultCandidateList(empty) == nullptr);

  assert(ResultCandidateMenuTitle(3) == "Choose Result (3)");

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

  RecognitionPayload missing_commit_payload;
  missing_commit_payload.commit_text = "not present";
  missing_commit_payload.candidates = {
      Candidate{"raw transcript", CandidateSource::Raw},
      Candidate{"polished", CandidateSource::Llm},
  };
  auto missing_commit_candidates = BuildResultCandidateList(missing_commit_payload);
  assert(missing_commit_candidates != nullptr);
  assert(missing_commit_candidates->totalSize() == 2);
  assert(missing_commit_candidates->globalCursorIndex() == 0);

  RecognitionPayload cancel_payload;
  cancel_payload.candidates = {Candidate{"", CandidateSource::Cancel}};
  auto cancel_candidates = BuildResultCandidateList(
      cancel_payload, [&selected_candidates](fcitx::InputContext *input_context,
                                             const Candidate &candidate) {
        assert(input_context == nullptr);
        selected_candidates.push_back(candidate);
      });
  assert(cancel_candidates != nullptr);
  assert(cancel_candidates->totalSize() == 1);
  assert(cancel_candidates->globalCursorIndex() == 0);
#ifdef VINPUT_FCITX5_CORE_HAVE_CANDIDATE_COMMENT
  assert(cancel_candidates->candidateFromAll(0).comment().toString() == "Cancel");
#endif
  cancel_candidates->candidateFromAll(0).select(nullptr);
  assert(selected_candidates.size() == 2);
  assert(selected_candidates[1].text.empty());
  assert(selected_candidates[1].source == CandidateSource::Cancel);

  return 0;
}
