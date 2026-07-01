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
  assert(candidates->candidateFromAll(0).comment().toString() == "ASR raw");
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

  RecognitionPayload asr_payload;
  asr_payload.commit_text = "asr choice";
  asr_payload.candidates = {Candidate{"asr choice", CandidateSource::Asr}};
  auto asr_candidates = BuildResultCandidateList(asr_payload);
  assert(asr_candidates != nullptr);
#ifdef VINPUT_FCITX5_CORE_HAVE_CANDIDATE_COMMENT
  assert(asr_candidates->candidateFromAll(0).comment().toString() == "ASR");
#endif

  RecognitionPayload mixed_payload;
  mixed_payload.commit_text = "second polished";
  mixed_payload.candidates = {
      Candidate{"raw transcript", CandidateSource::Raw},
      Candidate{"asr transcript", CandidateSource::Asr},
      Candidate{"first polished", CandidateSource::Llm},
      Candidate{"second polished", CandidateSource::Llm},
  };
  auto mixed_candidates = BuildResultCandidateList(mixed_payload);
  assert(mixed_candidates != nullptr);
  assert(mixed_candidates->globalCursorIndex() == 3);
#ifdef VINPUT_FCITX5_CORE_HAVE_CANDIDATE_COMMENT
  assert(mixed_candidates->candidateFromAll(0).comment().toString() == "ASR raw");
  assert(mixed_candidates->candidateFromAll(1).comment().toString() == "ASR");
  assert(mixed_candidates->candidateFromAll(2).comment().toString() == "LLM 1");
  assert(mixed_candidates->candidateFromAll(3).comment().toString() == "LLM 2");
#endif

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

  RecognitionPayload paged_payload;
  paged_payload.commit_text = "choice 6";
  paged_payload.candidates = {
      Candidate{"choice 1", CandidateSource::Llm},
      Candidate{"choice 2", CandidateSource::Llm},
      Candidate{"choice 3", CandidateSource::Llm},
      Candidate{"choice 4", CandidateSource::Llm},
      Candidate{"choice 5", CandidateSource::Llm},
      Candidate{"choice 6", CandidateSource::Llm},
  };
  auto paged_candidates = BuildResultCandidateList(paged_payload);
  assert(paged_candidates != nullptr);
  assert(ResultCandidateMenuTitle(paged_candidates->totalSize()) ==
         "Choose Result (6)");
  assert(paged_candidates->totalSize() == 6);
  assert(paged_candidates->size() == 5);
  assert(paged_candidates->pageSize() == 5);
  assert(paged_candidates->layoutHint() == fcitx::CandidateLayoutHint::Vertical);
  assert(paged_candidates->globalCursorIndex() == 5);
  assert(paged_candidates->candidateFromAll(5).text().toString() == "choice 6");

  return 0;
}
