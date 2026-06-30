#include "vinput_fcitx_bridge/fcitx_candidates.h"

#include <fcitx/text.h>

#include <utility>

namespace vinput_fcitx_bridge {
namespace {

constexpr int kResultMenuPageSize = 5;

class ResultCandidateWord final : public fcitx::CandidateWord {
public:
  ResultCandidateWord(std::string text, std::string comment)
      : fcitx::CandidateWord(fcitx::Text(std::move(text))) {
    if (!comment.empty()) {
      setComment(fcitx::Text(std::move(comment)));
    }
  }

  void select(fcitx::InputContext * /*input_context*/) const override {}
};

} // namespace

std::string ResultCandidateComment(const Candidate &candidate, std::size_t llm_index) {
  switch (candidate.source) {
  case CandidateSource::Raw:
    return "ASR raw";
  case CandidateSource::Asr:
    return "ASR";
  case CandidateSource::Llm:
    return "LLM " + std::to_string(llm_index);
  case CandidateSource::Cancel:
    return "Cancel";
  }
  return {};
}

std::unique_ptr<fcitx::CommonCandidateList>
BuildResultCandidateList(const RecognitionPayload &payload) {
  if (payload.candidates.empty()) {
    return nullptr;
  }

  auto candidate_list = std::make_unique<fcitx::CommonCandidateList>();
  candidate_list->setPageSize(kResultMenuPageSize);
  candidate_list->setLayoutHint(fcitx::CandidateLayoutHint::Vertical);
  candidate_list->setCursorPositionAfterPaging(
      fcitx::CursorPositionAfterPaging::ResetToFirst);

  int cursor_index = 0;
  std::size_t llm_index = 0;
  for (const auto &candidate : payload.candidates) {
    if (candidate.source == CandidateSource::Llm) {
      ++llm_index;
    }
    if (candidate.text == payload.commit_text) {
      cursor_index = candidate_list->totalSize();
    }
    candidate_list->append<ResultCandidateWord>(
        candidate.text, ResultCandidateComment(candidate, llm_index));
  }
  candidate_list->setGlobalCursorIndex(cursor_index);
  return candidate_list;
}

} // namespace vinput_fcitx_bridge
