#include "vinput_fcitx_bridge/fcitx_candidates.h"

#include <fcitx/inputcontext.h>
#include <fcitx/inputpanel.h>
#include <fcitx/text.h>
#include <fcitx/userinterface.h>

#include <string_view>
#include <utility>

namespace vinput_fcitx_bridge {
namespace {

constexpr int kResultMenuPageSize = 5;

class ResultCandidateWord final : public fcitx::CandidateWord {
public:
  ResultCandidateWord(Candidate candidate, std::string_view comment,
                      ResultCandidateSelectCallback on_select)
      : fcitx::CandidateWord(fcitx::Text(candidate.text)),
        candidate_(std::move(candidate)), on_select_(std::move(on_select)) {
#ifdef VINPUT_FCITX5_CORE_HAVE_CANDIDATE_COMMENT
    if (!comment.empty()) {
      setComment(fcitx::Text(std::string(comment)));
    }
#else
    (void)comment;
#endif
  }

  void select(fcitx::InputContext *input_context) const override {
    if (on_select_) {
      on_select_(input_context, candidate_);
    }
  }

private:
  Candidate candidate_;
  ResultCandidateSelectCallback on_select_;
};

} // namespace

std::string ResultCandidateMenuTitle(std::size_t count) {
  return "Choose Result (" + std::to_string(count) + ")";
}

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

void ApplyResultCandidateSelection(fcitx::InputContext *input_context,
                                   const Candidate &candidate) {
  if (input_context == nullptr) {
    return;
  }

  fcitx::Text empty;
  input_context->inputPanel().setAuxUp(empty);
  input_context->inputPanel().setPreedit(empty);
  input_context->inputPanel().setCandidateList({});
  input_context->updatePreedit();
  input_context->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);

  if (candidate.source == CandidateSource::Cancel || candidate.text.empty()) {
    return;
  }

  input_context->commitString(candidate.text);
}

std::unique_ptr<fcitx::CommonCandidateList>
BuildResultCandidateList(const RecognitionPayload &payload,
                         const ResultCandidateSelectCallback &on_select) {
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
        candidate, ResultCandidateComment(candidate, llm_index), on_select);
  }
  candidate_list->setGlobalCursorIndex(cursor_index);
  return candidate_list;
}

} // namespace vinput_fcitx_bridge
