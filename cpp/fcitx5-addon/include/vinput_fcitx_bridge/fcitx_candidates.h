#pragma once

#include "vinput_fcitx_bridge/recognition_payload.h"

#include <fcitx/candidatelist.h>

#include <functional>
#include <memory>
#include <string>

namespace vinput_fcitx_bridge {

using ResultCandidateSelectCallback =
    std::function<void(fcitx::InputContext *, const Candidate &)>;

std::string ResultCandidateComment(const Candidate &candidate, std::size_t llm_index);

std::string ResultCandidateMenuTitle(std::size_t count);

void ClearResultCandidateMenu(fcitx::InputContext *input_context);

void ApplyResultCandidateSelection(fcitx::InputContext *input_context,
                                   const Candidate &candidate);

std::unique_ptr<fcitx::CommonCandidateList> BuildResultCandidateList(
    const RecognitionPayload &payload,
    const ResultCandidateSelectCallback &on_select = ApplyResultCandidateSelection);

} // namespace vinput_fcitx_bridge
