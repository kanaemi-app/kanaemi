// The engine Fcitx5 sees. It hands each event to the Rust library over the
// C interface in ffi.rs, and shows what comes back. Nothing is decided here.

#include <dlfcn.h>

#include <cstddef>
#include <cstdint>
#include <memory>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include <fcitx-utils/eventdispatcher.h>
#include <fcitx-utils/event.h>
#include <fcitx-utils/i18n.h>
#include <fcitx-utils/key.h>
#include <fcitx/action.h>
#include <fcitx/addonfactory.h>
#include <fcitx/addoninstance.h>
#include <fcitx/addonmanager.h>
#include <fcitx/candidatelist.h>
#include <fcitx/event.h>
#include <fcitx/inputcontext.h>
#include <fcitx/inputcontextproperty.h>
#include <fcitx/inputmethodengine.h>
#include <fcitx/inputpanel.h>
#include <fcitx/instance.h>
#include <fcitx/statusarea.h>
#include <fcitx/userinterfacemanager.h>

namespace {

struct Item {
    const char *text;
    size_t text_len;
    const char *comment;
    size_t comment_len;
};

struct Callbacks {
    void (*commit)(void *ic, const char *text, size_t len);
    void (*preedit)(void *ic, const char *text, size_t len, size_t cursor);
    void (*candidates)(void *ic, const Item *items, size_t count,
                       size_t selected, const char *aside, size_t aside_len);
    void (*hide_candidates)(void *ic);
    void (*forward)(void *ic, uint32_t keysym, uint32_t state);
    void (*delete_before)(void *ic, size_t chars);
    void (*indicator)(void *ic, const char *label, size_t len);
    void (*wake)(void *engine);
};

struct Addon;

// Which frontend serves the context. Not frontendName(), which Fcitx5 has
// only from 5.0.22: the packages are built against an older one.
std::string_view frontend(const fcitx::InputContext *ic) {
    return ic->frontend();
}

} // namespace

extern "C" {
Addon *kanaemi_fcitx5_new(const Callbacks *callbacks, void *engine,
                          const char *library);
void kanaemi_fcitx5_free(Addon *addon);
void kanaemi_fcitx5_create(Addon *addon, void *ic);
void kanaemi_fcitx5_destroy(Addon *addon, void *ic);
bool kanaemi_fcitx5_key(Addon *addon, void *ic, uint32_t keysym, uint32_t code,
                        uint32_t state, bool release);
void kanaemi_fcitx5_focus_in(Addon *addon, void *ic, uint64_t flags,
                             const char *program, bool held_modifiers);
void kanaemi_fcitx5_set_capabilities(Addon *addon, void *ic, uint64_t flags);
void kanaemi_fcitx5_focus_out(Addon *addon, void *ic, bool committable);
void kanaemi_fcitx5_reset(Addon *addon, void *ic);
void kanaemi_fcitx5_select(Addon *addon, void *ic, size_t index);
void kanaemi_fcitx5_serve(Addon *addon);
void kanaemi_fcitx5_open_settings(Addon *addon);
}

namespace {

/// How long the mode shows by the caret.
constexpr uint64_t INDICATOR_VISIBLE_USEC = 800000;

fcitx::InputContext *context(void *ic) {
    return static_cast<fcitx::InputContext *>(ic);
}

/// The file this add-on was loaded from.
std::string library() {
    Dl_info info{};
    if (dladdr(reinterpret_cast<void *>(&library), &info) == 0 ||
        info.dli_fname == nullptr) {
        return {};
    }
    return info.dli_fname;
}

class Engine;

/// Tells the Rust library of each input context it makes and destroys.
class State : public fcitx::InputContextProperty {
public:
    State(Addon *addon, fcitx::InputContext *ic) : addon_(addon), ic_(ic) {
        kanaemi_fcitx5_create(addon_, ic_);
    }
    ~State() override { kanaemi_fcitx5_destroy(addon_, ic_); }

private:
    Addon *addon_;
    fcitx::InputContext *ic_;
};

/// A candidate the user can click, which goes back by its position on the
/// page.
class Candidate : public fcitx::CandidateWord {
public:
    Candidate(Addon *addon, size_t index, std::string text, std::string comment)
        : fcitx::CandidateWord(fcitx::Text(std::move(text))), addon_(addon),
          index_(index) {
        if (comment.empty()) {
            return;
        }
#ifdef KANAEMI_FCITX5_CANDIDATE_COMMENT
        setComment(fcitx::Text(std::move(comment)));
#else
        // Fcitx5 before 5.1.9 has no comment for a candidate: it follows the
        // candidate in its text, set apart as the IBus engine sets it.
        auto label = this->text();
        label.append("　" + comment);
        setText(std::move(label));
#endif
    }

    void select(fcitx::InputContext *ic) const override {
        kanaemi_fcitx5_select(addon_, ic, index_);
    }

private:
    Addon *addon_;
    size_t index_;
};

class Engine : public fcitx::InputMethodEngineV2 {
public:
    explicit Engine(fcitx::Instance *instance)
        : instance_(instance),
          factory_([this](fcitx::InputContext &ic) -> State * {
              return addon_ ? new State(addon_, &ic) : nullptr;
          }) {
        dispatcher_.attach(&instance_->eventLoop());
        current_ = this;
        addon_ = kanaemi_fcitx5_new(&CALLBACKS, this, library().c_str());
        instance_->inputContextManager().registerProperty("kanaemiState",
                                                          &factory_);
        settings_.setShortText("設定を開く…");
        settings_.setIcon("preferences-system");
        settings_.connect<fcitx::SimpleAction::Activated>(
            [this](fcitx::InputContext *) {
                kanaemi_fcitx5_open_settings(addon_);
            });
        instance_->userInterfaceManager().registerAction("kanaemi-settings",
                                                         &settings_);
        capabilities_ = instance_->watchEvent(
            fcitx::EventType::InputContextCapabilityChanged,
            fcitx::EventWatcherPhase::Default, [this](fcitx::Event &event) {
                auto &changed =
                    static_cast<fcitx::CapabilityChangedEvent &>(event);
                auto *ic = changed.inputContext();
                if (addon_ && ic->hasFocus() &&
                    instance_->inputMethodEngine(ic) == this) {
                    kanaemi_fcitx5_set_capabilities(
                        addon_, ic, static_cast<uint64_t>(changed.newFlags()));
                }
            });
    }

    ~Engine() override {
        // The contexts tell the library they go, so before it does.
        factory_.unregister();
        capabilities_.reset();
        indicatorTimer_.reset();
        kanaemi_fcitx5_free(addon_);
        addon_ = nullptr;
        dispatcher_.detach();
        current_ = nullptr;
    }

    bool ready() const { return addon_ != nullptr; }

    void keyEvent(const fcitx::InputMethodEntry &,
                  fcitx::KeyEvent &event) override {
        auto *ic = event.inputContext();
        ic->propertyFor(&factory_);
        auto key = event.rawKey();
        if (kanaemi_fcitx5_key(addon_, ic, key.sym(), key.code(),
                               static_cast<uint32_t>(key.states()),
                               event.isRelease())) {
            event.filterAndAccept();
        }
    }

    void activate(const fcitx::InputMethodEntry &,
                  fcitx::InputContextEvent &event) override {
        auto *ic = event.inputContext();
        ic->propertyFor(&factory_);
        ic->statusArea().addAction(fcitx::StatusGroup::InputMethod,
                                   &settings_);
        // Wayland's second input method protocol forwards a key through a
        // virtual keyboard, with the modifiers held down instead of the
        // key's own.
        kanaemi_fcitx5_focus_in(
            addon_, ic, static_cast<uint64_t>(ic->capabilityFlags()),
            ic->program().c_str(), frontend(ic) == "wayland_v2");
    }

    void deactivate(const fcitx::InputMethodEntry &,
                    fcitx::InputContextEvent &event) override {
        auto *ic = event.inputContext();
        // Wayland's input method protocols let the field go before the focus
        // leaves it, and drop what is committed after.
        bool committable =
            event.type() != fcitx::EventType::InputContextFocusOut ||
            (frontend(ic) != "wayland" &&
             frontend(ic) != "wayland_v2");
        kanaemi_fcitx5_focus_out(addon_, ic, committable);
        // What is left of the panel goes with the focus.
        ic->inputPanel().reset();
        ic->updatePreedit();
        ic->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
    }

    void reset(const fcitx::InputMethodEntry &,
               fcitx::InputContextEvent &event) override {
        kanaemi_fcitx5_reset(addon_, event.inputContext());
    }

private:
    static void commit(void *ic, const char *text, size_t len) {
        context(ic)->commitString(std::string(text, len));
    }

    /// The preedit, in the application where it can show one. Its marks
    /// show its state, so it is neither underlined nor committed as it is
    /// when the focus goes: what is typed is committed by the library.
    static void preedit(void *ic, const char *text, size_t len,
                        size_t cursor) {
        auto *c = context(ic);
        fcitx::Text preedit;
        if (len > 0) {
            preedit.append(std::string(text, len),
                           fcitx::TextFormatFlag::DontCommit);
            preedit.setCursor(static_cast<int>(cursor));
        }
        if (c->capabilityFlags().test(fcitx::CapabilityFlag::Preedit)) {
            c->inputPanel().setClientPreedit(preedit);
            c->inputPanel().setPreedit(fcitx::Text());
        } else {
            c->inputPanel().setPreedit(preedit);
        }
        c->updatePreedit();
        c->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
    }

    static void candidates(void *ic, const Item *items, size_t count,
                           size_t selected, const char *aside,
                           size_t aside_len) {
        auto *c = context(ic);
        auto *engine = current_;
        if (!engine) {
            return;
        }
        // The candidates take the place by the caret.
        engine->indicatorShown_ = false;
        auto list = std::make_unique<fcitx::CommonCandidateList>();
        list->setPageSize(static_cast<int>(count > 0 ? count : 1));
        list->setLayoutHint(fcitx::CandidateLayoutHint::Vertical);
        std::vector<std::string> labels;
        for (size_t i = 0; i < count; ++i) {
            labels.push_back(std::to_string(i + 1) + ". ");
        }
        list->setLabels(labels);
        for (size_t i = 0; i < count; ++i) {
            list->append<Candidate>(
                engine->addon_, i,
                std::string(items[i].text, items[i].text_len),
                std::string(items[i].comment, items[i].comment_len));
        }
        if (count > 0) {
            list->setGlobalCursorIndex(static_cast<int>(selected));
        }
        c->inputPanel().setCandidateList(std::move(list));
        c->inputPanel().setAuxUp(fcitx::Text(std::string(aside, aside_len)));
        c->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
    }

    static void hideCandidates(void *ic) {
        auto *c = context(ic);
        auto *engine = current_;
        c->inputPanel().setCandidateList(nullptr);
        // The mode shown there for a moment stays.
        if (!engine || !engine->indicatorShown_) {
            c->inputPanel().setAuxUp(fcitx::Text());
        }
        c->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
    }

    static void deleteBefore(void *ic, size_t chars) {
        auto count = static_cast<int>(chars);
        context(ic)->deleteSurroundingText(-count, count);
    }

    static void forward(void *ic, uint32_t keysym, uint32_t state) {
        fcitx::Key key(static_cast<fcitx::KeySym>(keysym),
                       fcitx::KeyStates(state));
        context(ic)->forwardKey(key, false);
        context(ic)->forwardKey(key, true);
    }

    /// Shows the mode by the caret, and hides it once a moment has passed,
    /// unless the candidates took its place meanwhile.
    static void indicator(void *ic, const char *label, size_t len) {
        auto *c = context(ic);
        auto *engine = current_;
        if (!engine) {
            return;
        }
        c->inputPanel().setAuxUp(fcitx::Text(std::string(label, len)));
        c->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
        engine->indicatorShown_ = true;
        auto ref = c->watch();
        engine->indicatorTimer_ =
            engine->instance_->eventLoop().addTimeEvent(
                CLOCK_MONOTONIC, fcitx::now(CLOCK_MONOTONIC) +
                                     INDICATOR_VISIBLE_USEC,
                0, [engine, ref](fcitx::EventSourceTime *, uint64_t) {
                    if (engine->indicatorShown_) {
                        engine->indicatorShown_ = false;
                        if (auto *c = ref.get()) {
                            c->inputPanel().setAuxUp(fcitx::Text());
                            c->updateUserInterface(
                                fcitx::UserInterfaceComponent::InputPanel);
                        }
                    }
                    return true;
                });
    }

    static void wake(void *engine) {
        auto *self = static_cast<Engine *>(engine);
        self->dispatcher_.schedule([self]() {
            if (self->addon_) {
                kanaemi_fcitx5_serve(self->addon_);
            }
        });
    }

    static constexpr Callbacks CALLBACKS{
        commit,  preedit,      candidates, hideCandidates,
        forward, deleteBefore, indicator,  wake,
    };

    // Fcitx5 makes one engine of an add-on.
    static inline Engine *current_ = nullptr;

    fcitx::Instance *instance_;
    Addon *addon_ = nullptr;
    fcitx::FactoryFor<State> factory_;
    fcitx::EventDispatcher dispatcher_;
    fcitx::SimpleAction settings_;
    std::unique_ptr<fcitx::HandlerTableEntry<fcitx::EventHandler>>
        capabilities_;
    std::unique_ptr<fcitx::EventSourceTime> indicatorTimer_;
    bool indicatorShown_ = false;
};

class Factory : public fcitx::AddonFactory {
public:
    fcitx::AddonInstance *create(fcitx::AddonManager *manager) override {
        auto engine = std::make_unique<Engine>(manager->instance());
        return engine->ready() ? engine.release() : nullptr;
    }
};

} // namespace

extern "C" fcitx::AddonFactory *kanaemi_fcitx5_factory() {
    static Factory factory;
    return &factory;
}
