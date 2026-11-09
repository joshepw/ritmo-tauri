(function () {
  function api() {
    return window.__TAURI__;
  }

  async function invoke(command, args) {
    const tauri = api();
    if (!tauri) throw new Error('Abre Ritmo desde la app de escritorio.');
    return tauri.core.invoke(command, args || {});
  }

  function messageOf(error) {
    if (typeof error === 'string') return error;
    if (error && typeof error.message === 'string') return error.message;
    return 'No se pudo completar la acción.';
  }

  function settingsSurface() {
    return /settings\.html$/.test(window.location.pathname);
  }

  function toast(component, text, color, abbr) {
    component.setState({ toast: { text: messageOf(text), color: color || '#0a84ff', abbr: abbr || '·' } });
    clearTimeout(component.tt);
    component.tt = setTimeout(() => component.setState({ toast: null }), 2800);
  }

  function applySnapshot(component, snap) {
    const first = !component._booted;
    component._booted = true;
    const patch = {
      oauthConfigured: snap.oauthConfigured,
      connected: snap.connected,
      accountName: snap.accountName,
      accountEmail: snap.accountEmail,
      accountInitials: snap.accountInitials,
      accountStatus: snap.accountStatus,
      accountStatusColor: snap.accountStatusColor,
      weekdayLabel: snap.weekdayLabel,
      dateLong: snap.dateLong,
      menuClock: snap.menuClock,
      nowMark: snap.nowMark,
      barMeetingLabel: snap.barMeetingLabel,
      showBarTimer: snap.showBarTimer,
      showBarMeeting: snap.showBarMeeting,
      agendaToday: snap.today || [],
      agendaDays: snap.days || [],
      nextTitle: snap.nextTitle,
      nextMeta: snap.nextMeta,
      nextColor: snap.nextColor,
      nextAbbr: snap.nextAbbr,
      nextJoinUrl: snap.nextJoinUrl,
      taskItems: snap.tasks || [],
      clickupItems: snap.clickupTasks || [],
      clickupGroups: snap.clickupGroups || [],
      noteItems: snap.notes || [],
      calendars: snap.calendars || [],
      suggestions: snap.suggestions || [],
      p: snap.preferences,
      geminiKeySet: snap.geminiKeySet,
      factorialKeySet: !!snap.factorialKeySet,
      clickupKeySet: !!snap.clickupKeySet,
      geminiKeyMask: snap.geminiKeyMask || '',
      factorialKeyMask: snap.factorialKeyMask || '',
      clickupKeyMask: snap.clickupKeyMask || '',
      remainingSecs: snap.pomodoro.remainingSecs,
      durationSecs: snap.pomodoro.durationSecs,
      running: snap.pomodoro.running,
      mode: snap.pomodoro.mode,
      session: snap.pomodoro.session,
      focusTaskId: snap.pomodoro.focusTaskId,
      completed: snap.pomodoro.completed || {},
      volume: snap.preferences.volume,
      track: snap.preferences.track,
      autoSound: snap.preferences.autoSound,
      breathTech: snap.preferences.breathTech,
      breathSound: snap.preferences.breathSound,
      breathCycles: snap.preferences.breathCycles || 6,
      eventsLoading: !!snap.eventsLoading,
      tasksLoading: !!snap.tasksLoading,
      clickupLoading: !!snap.clickupLoading,
    };
    if (first || (snap.preferences && snap.preferences.chatUi === 'Enlace')) {
      patch.chatOpen = false;
    }
    if (first) patch.isApp = true;
    component._agendaStamp = JSON.stringify([patch.agendaToday, patch.agendaDays]);
    component.setState(patch, () => component.afterRemote(false));
  }

  function applyTick(component, tick) {
    const patch = {
      remainingSecs: tick.remainingSecs,
      durationSecs: tick.durationSecs,
      running: tick.running,
      mode: tick.mode,
      session: tick.session,
      focusTaskId: tick.focusTaskId,
      completed: tick.completed || {},
      menuClock: tick.menuClock,
      weekdayLabel: tick.weekdayLabel,
      dateLong: tick.dateLong,
      nowMark: tick.nowMark,
      barMeetingLabel: tick.barMeetingLabel,
      showBarTimer: tick.showBarTimer,
      showBarMeeting: tick.showBarMeeting,
      nextTitle: tick.nextTitle,
      nextMeta: tick.nextMeta,
      nextColor: tick.nextColor,
      nextAbbr: tick.nextAbbr,
      nextJoinUrl: tick.nextJoinUrl,
    };
    if (Array.isArray(tick.today)) {
      const stamp = JSON.stringify([tick.today, tick.days || []]);
      if (stamp !== component._agendaStamp) {
        component._agendaStamp = stamp;
        patch.agendaToday = tick.today;
        patch.agendaDays = tick.days || [];
      }
    }
    component.setState(patch, () => component.afterRemote(!!tick.chime));
  }

  async function reload(component) {
    const snap = await invoke('snapshot');
    applySnapshot(component, snap);
    return snap;
  }

  const Ritmo = {
    async boot(component) {
      if (!api()) {
        component.setState({
          accountStatus: 'Abre Ritmo desde la app de escritorio',
          accountEmail: 'Esta vista necesita la app',
        });
        return;
      }
      document.body.classList.add('app');
      component.setState({ booting: true, eventsLoading: true, tasksLoading: true });
      try {
        await reload(component);
        await api().event.listen('ritmo-snapshot', () => {
          reload(component).catch((error) => toast(component, error));
        });
        await api().event.listen('ritmo-notice', (event) => toast(component, event.payload, '#ff9f0a', 'F'));
        if (!settingsSurface()) {
          await api().event.listen('ritmo-tick', (event) => applyTick(component, event.payload));
        }
      } catch (error) {
        component.setState({ aiLoading: false, eventsLoading: false, tasksLoading: false });
        toast(component, error);
      } finally {
        component.setState({ booting: false });
      }
      setTimeout(() => Ritmo.checkUpdate(component, true), 2500);
    },
    async checkUpdate(component, silent) {
      if (component.state.updateChecking || component.state.updateInstalling) return;
      component.setState({ updateChecking: true });
      try {
        const result = await invoke('check_update');
        component.setState({
          updateChecking: false,
          appVersion: result.current || component.state.appVersion,
          updateAvailable: !!result.available,
          updateVersion: result.version || '',
          updateNotes: result.notes || '',
        });
        if (!silent) {
          toast(component, result.available ? 'Hay una versión nueva: ' + result.version : 'Ya tienes la última versión.');
        } else if (result.available && !settingsSurface()) {
          toast(component, 'Hay una actualización: ' + result.version + '. Instálala en Preferencias.');
        }
      } catch (error) {
        component.setState({ updateChecking: false });
        if (!silent) toast(component, error);
      }
    },
    async installUpdate(component) {
      if (component.state.updateInstalling) return;
      component.setState({ updateInstalling: true });
      toast(component, 'Descargando la actualización…');
      try {
        await invoke('install_update');
      } catch (error) {
        component.setState({ updateInstalling: false });
        toast(component, error);
      }
    },
    async patch(component, patch) {
      try {
        const snap = await invoke('set_preferences', { patch });
        applySnapshot(component, snap);
      } catch (error) {
        toast(component, error);
      }
    },
    async signIn(component) {
      if (component.state.signingIn) return;
      component.setState({ signingIn: true });
      toast(component, 'Se abrirá el navegador para entrar con Google.');
      try {
        applySnapshot(component, await invoke('sign_in'));
      } catch (error) {
        toast(component, error);
      } finally {
        component.setState({ signingIn: false });
      }
    },
    async signOut(component) {
      try {
        applySnapshot(component, await invoke('sign_out'));
      } catch (error) {
        toast(component, error);
      }
    },
    async calendar(component, id, visible) {
      try {
        applySnapshot(component, await invoke('set_calendar_visible', { id, visible }));
      } catch (error) {
        toast(component, error);
      }
    },
    async addTask(component, title) {
      if (component.state.taskAdding) return;
      component.setState({ taskAdding: true });
      try {
        const snap = await invoke('add_task', { title });
        applySnapshot(component, snap);
        const created = (snap.tasks || []).find((task) => !task.done && task.title === title);
        component.setState({ taskAdding: false, draft: '', tab: 'hoy', freshTaskId: created ? created.id : null });
        clearTimeout(component._freshTask);
        component._freshTask = setTimeout(() => {
          if (component.state.freshTaskId) component.setState({ freshTaskId: null });
        }, 700);
      } catch (error) {
        component.setState({ taskAdding: false });
        toast(component, error);
      }
    },
    async toggleTask(component, taskId) {
      const pulse = component.state.taskPulse;
      if (pulse && pulse.id === taskId && pulse.phase === 'loading') return;
      component.setState({ taskPulse: { id: taskId, phase: 'loading' } });
      try {
        const snap = await invoke('toggle_task', { taskId });
        const clickup = String(taskId).indexOf('clickup:') === 0;
        const items = clickup ? snap.clickupTasks || [] : snap.tasks || [];
        const updated = items.find((task) => task.id === taskId);
        const marked = clickup ? !updated || !!updated.done : !!(updated && updated.done);
        if (!marked) {
          applySnapshot(component, snap);
          component.setState({ taskPulse: null });
          return;
        }
        component.setState({ taskPulse: { id: taskId, phase: 'done' } });
        clearTimeout(component._taskPulse);
        component._taskPulse = setTimeout(() => {
          applySnapshot(component, snap);
          if (component.state.taskPulse && component.state.taskPulse.id === taskId) {
            component.setState({ taskPulse: null });
          }
        }, 360);
      } catch (error) {
        component.setState({ taskPulse: { id: taskId, phase: 'error' } });
        toast(component, error);
        clearTimeout(component._taskPulse);
        component._taskPulse = setTimeout(() => {
          if (component.state.taskPulse && component.state.taskPulse.id === taskId) {
            component.setState({ taskPulse: null });
          }
        }, 460);
      }
    },
    async addNote(component, title, body) {
      try {
        applySnapshot(component, await invoke('add_note', { title, body }));
      } catch (error) {
        toast(component, error);
      }
    },
    async pomodoro(component, action, taskId) {
      try {
        applySnapshot(component, await invoke('pomodoro_action', { action, taskId: taskId || null }));
      } catch (error) {
        toast(component, error);
      }
    },
    async openMeeting(component, url, meta) {
      if (!url) return;
      try {
        const opened = await invoke('open_meeting', { url });
        toast(component, opened.message, meta && meta.color, meta && meta.abbr);
      } catch (error) {
        toast(component, error);
      }
    },
    async summary(component) {
      component.setState({ aiLoading: true });
      try {
        const result = await invoke('assistant_summary', { manual: true });
        component.setState({ aiSummary: result.text, aiLoading: false });
      } catch (error) {
        component.setState({ aiLoading: false });
        toast(component, error);
      }
    },
    async ask(component, question) {
      component.setState((state) => ({
        aiDraft: '',
        aiAsking: true,
        aiChat: [...(state.aiChat || []), { role: 'user', text: question }],
      }));
      try {
        const result = await invoke('assistant_ask', { question });
        component.setState((state) => ({
          aiAsking: false,
          aiChat: [...(state.aiChat || []), { role: 'ai', text: result.text }],
        }));
      } catch (error) {
        component.setState((state) => ({
          aiAsking: false,
          aiChat: [...(state.aiChat || []), { role: 'ai', text: String(error) }],
        }));
      }
    },
    async setGeminiKey(component) {
      const key = window.prompt('Clave de API de Gemini');
      if (!key) return;
      try {
        applySnapshot(component, await invoke('set_gemini_key', { key }));
        toast(component, 'Clave guardada en el llavero.');
      } catch (error) {
        toast(component, error);
      }
    },
    async setFactorialKey(component) {
      const key = (component.state.factorialDraft || '').trim();
      if (!key) {
        toast(component, 'Pega la clave de Factorial en el campo.');
        return;
      }
      try {
        applySnapshot(component, await invoke('set_factorial_key', { key }));
        component.setState({ factorialDraft: '' });
        toast(component, 'Clave de Factorial guardada.');
      } catch (error) {
        toast(component, error);
      }
    },
    async clearFactorialKey(component) {
      try {
        applySnapshot(component, await invoke('clear_factorial_key'));
        component.setState({ factorialDraft: '' });
        toast(component, 'Clave de Factorial quitada.');
      } catch (error) {
        toast(component, error);
      }
    },
    async setClickupKey(component) {
      const key = (component.state.clickupDraft || '').trim();
      if (!key) {
        toast(component, 'Pega la clave de ClickUp en el campo.');
        return;
      }
      try {
        applySnapshot(component, await invoke('set_clickup_key', { key }));
        component.setState({ clickupDraft: '' });
        toast(component, 'Clave de ClickUp guardada.');
      } catch (error) {
        toast(component, error);
      }
    },
    async clearClickupKey(component) {
      try {
        applySnapshot(component, await invoke('clear_clickup_key'));
        component.setState({ clickupDraft: '' });
        toast(component, 'Clave de ClickUp quitada.');
      } catch (error) {
        toast(component, error);
      }
    },
    async clearGeminiKey(component) {
      try {
        applySnapshot(component, await invoke('clear_gemini_key'));
      } catch (error) {
        toast(component, error);
      }
    },
    async previewAlert(component) {
      try {
        const result = await invoke('preview_notification');
        toast(component, (result && result.text) || 'Aviso enviado.');
      } catch (error) {
        toast(component, messageOf(error));
      }
    },
    async launchChat(component) {
      try {
        await invoke('open_meeting', { url: 'https://chat.google.com/' });
      } catch (error) {
        toast(component, error);
      }
    },
  };

  window.Ritmo = Ritmo;

  window.RitmoDefine = function (DCLogic) {
    return class Component extends DCLogic {
      state = {
        prefsOpen: false, prefsTab: 'cuenta', connected: false, isApp: false,
        accountName: 'Sin cuenta conectada', accountEmail: 'Conecta tu cuenta de Google',
        accountInitials: 'G', accountStatus: 'Desconectado', accountStatusColor: 'rgba(235,235,245,.5)',
        weekdayLabel: '', dateLong: '', menuClock: '', nowMark: '', barMeetingLabel: '',
        showBarTimer: true, showBarMeeting: false,
        p: { login: true, barTimer: true, barMeeting: true, barMeetMin: '1 h', range: '7 días', linkRows: [
          { key: 'openZoom', label: 'Abrir Zoom', value: 'nativeApp' },
          { key: 'openTeams', label: 'Abrir Microsoft Teams', value: 'defaultBrowser' },
          { key: 'openWebex', label: 'Abrir Webex', value: 'defaultBrowser' },
          { key: 'openMeet', label: 'Abrir Google Meet', value: 'chrome' },
          { key: 'openHangouts', label: 'Abrir Google Hangouts', value: 'chrome' },
          { key: 'openSlack', label: 'Abrir Slack', value: 'nativeApp' },
          { key: 'openJitsi', label: 'Abrir Jitsi', value: 'defaultBrowser' },
          { key: 'openWhereby', label: 'Abrir Whereby', value: 'defaultBrowser' },
          { key: 'openGoto', label: 'Abrir GoTo Meeting', value: 'nativeApp' },
          { key: 'openChime', label: 'Abrir Amazon Chime', value: 'nativeApp' },
          { key: 'openDiscord', label: 'Abrir Discord', value: 'nativeApp' },
          { key: 'openFacetime', label: 'Abrir FaceTime', value: 'nativeApp' },
          { key: 'openOther', label: 'Otros enlaces', value: 'defaultBrowser' },
        ], detectNotes: true, detectDesc: true, notify: '5 min', autoJoin: false, hideDeclined: true, focusMin: 25, breakMin: 5, longBreak: 15, autoBreak: true, pauseMeet: true, autoSound: true, volume: 40, track: 'brown', breathTech: 'coherente', breathSound: true, breathCycles: 6, aiDaily: true, aiTime: 'Al abrir', aiSuggest: true, aiNotes: true, svcCal: true, svcTasks: false, svcGemini: false, svcFactorial: false, svcClickup: false, chatUi: 'iframe', shortcut: '⌥ ⌘ Espacio' },
        agendaToday: [], agendaDays: [], taskItems: [], clickupItems: [], clickupGroups: [], noteItems: [], calendars: [], suggestions: [],
        nextTitle: 'Conecta tu cuenta', nextMeta: 'Google Calendar', nextColor: 'rgba(255,255,255,.18)', nextAbbr: '·', nextJoinUrl: '',
        mode: 'focus', running: false, remainingSecs: 25 * 60, durationSecs: 25 * 60, session: 1,
        focusTaskId: null, completed: {}, volume: 40, track: 'brown', autoSound: true,
        breathTech: 'coherente', breathSound: true, breathCycles: 6, toast: null,
        tab: 'hoy', musicOpen: false, taskMenuOpen: false, soundOn: false,
        aiOpen: false, aiSummary: '', aiLoading: false, aiAsking: false, aiDraft: '', aiChat: [],
        chatOpen: false,
        booting: false, signingIn: false,
        draft: '', factorialDraft: '', clickupDraft: '', taskAdding: false, taskHistory: false, freshTaskId: null, taskPulse: null, geminiKeySet: false, factorialKeySet: false, clickupKeySet: false, geminiKeyMask: '', factorialKeyMask: '', clickupKeyMask: '', clickupLoading: false, oauthConfigured: false,
        shortcutRecording: false,
        appVersion: '0.1.0', updateAvailable: false, updateVersion: '', updateNotes: '',
        updateChecking: false, updateInstalling: false,
      };

      componentDidMount() {
        if (window.__TAURI__) {
          document.body.classList.add('app');
          this.setState({ isApp: true });
        }
        Ritmo.boot(this);
        const events = window.__TAURI__ && window.__TAURI__.event;
        if (events) {
          events.listen('ritmo-surface', (event) => {
            if (event.payload === 'chat-closed' && this.state.chatOpen) this.setState({ chatOpen: false });
            if (event.payload === 'gemini-closed' && this.state.aiOpen) this.setState({ aiOpen: false });
            if (event.payload === 'settings-closed' && this.state.prefsOpen) this.setState({ prefsOpen: false });
            if (event.payload === 'open-notes') this.setState({ tab: 'notas' });
          });
        }
      }
      syncChatSurface() {
        if (settingsSurface()) return;
        const want = !!(this.state.chatOpen && this.state.p && this.state.p.chatUi === 'iframe' && window.__TAURI__);
        if (want === this._chatSurface) return;
        this._chatSurface = want;
        invoke(want ? 'show_chat_window' : 'hide_chat_window').catch((error) => toast(this, error));
      }
      componentDidUpdate() {
        this.syncChatSurface();
        if (this._wantBreathExit) {
          if (!this._breathTimer) {
            this._breathTimer = setTimeout(() => {
              this._breathTimer = null;
              this._breathOnScreen = false;
              this._wantBreathExit = false;
              this.forceUpdate();
            }, 620);
          }
          return;
        }
        if (this._breathTimer) {
          clearTimeout(this._breathTimer);
          this._breathTimer = null;
        }
      }
      componentWillUnmount() {
        clearTimeout(this._breathTimer);
        this.stopAudio();
        this.stopBreathAudio();
        this.stopShortcut();
        if (this._chatSurface) invoke('hide_chat_window').catch(() => {});
      }
      stopShortcut() {
        if (this._shortcutListen) {
          window.removeEventListener('keydown', this._shortcutListen, true);
          this._shortcutListen = null;
        }
      }
      captureShortcut() {
        if (this._shortcutListen) return;
        this.setState({ shortcutRecording: true });
        const onKey = (event) => {
          event.preventDefault();
          event.stopPropagation();
          if (event.key === 'Escape') {
            this.stopShortcut();
            this.setState({ shortcutRecording: false });
            return;
          }
          if (['Alt', 'Shift', 'Meta', 'Control'].includes(event.key)) return;
          const mods = [];
          if (event.shiftKey) mods.push('shift');
          if (event.ctrlKey) mods.push('control');
          if (event.altKey) mods.push('alt');
          if (event.metaKey) mods.push('super');
          if (!mods.length) return;
          const named = { ' ': 'Space', Spacebar: 'Space', Enter: 'Enter', Tab: 'Tab', ArrowUp: 'ArrowUp', ArrowDown: 'ArrowDown', ArrowLeft: 'ArrowLeft', ArrowRight: 'ArrowRight' };
          const key = named[event.key] || (/^[a-zA-Z]$/.test(event.key) ? event.key.toUpperCase() : (/^[0-9]$/.test(event.key) ? event.key : ''));
          if (!key) return;
          this.stopShortcut();
          this.setState({ shortcutRecording: false });
          Ritmo.patch(this, { shortcut: mods.concat(key).join('+') });
        };
        this._shortcutListen = onKey;
        window.addEventListener('keydown', onKey, true);
      }
      afterRemote(chime) {
        if (settingsSurface()) return;
        if (chime) this.chime();
        const state = this.state;
        if (state.soundOn) this.stopAudio();
        this.breathTick();
      }
      toast(text) { toast(this, text); }

      rem() { return this.state.remainingSecs || 0; }
      total() { return this.state.durationSecs || 1; }
      ensureCtx() {
        if (!this.ctx) this.ctx = new (window.AudioContext || window.webkitAudioContext)();
        return this.ctx;
      }
      noiseBuf(ctx, brown) {
        const len = ctx.sampleRate * 4, buffer = ctx.createBuffer(1, len, ctx.sampleRate), data = buffer.getChannelData(0);
        let last = 0;
        for (let i = 0; i < len; i++) {
          const white = Math.random() * 2 - 1;
          if (brown) { last = (last + 0.02 * white) / 1.02; data[i] = last * 3.5; }
          else data[i] = white;
        }
        return buffer;
      }
      startAudio(track) {
        this.stopAudio();
        const chosen = track || this.state.track;
        const ctx = this.ensureCtx();
        ctx.resume();
        const gain = ctx.createGain();
        gain.gain.value = this.state.volume / 100 * 0.5;
        gain.connect(ctx.destination);
        const nodes = [gain];
        if (chosen === 'brown' || chosen === 'rain') {
          const source = ctx.createBufferSource();
          source.buffer = this.noiseBuf(ctx, chosen === 'brown');
          source.loop = true;
          if (chosen === 'rain') {
            const filter = ctx.createBiquadFilter();
            filter.type = 'bandpass';
            filter.frequency.value = 1400;
            filter.Q.value = 0.4;
            const low = ctx.createBiquadFilter();
            low.type = 'lowpass';
            low.frequency.value = 5000;
            source.connect(filter);
            filter.connect(low);
            low.connect(gain);
            gain.gain.value *= 0.6;
          } else source.connect(gain);
          source.start();
          nodes.push(source);
        } else {
          const base = chosen === 'binaural' ? [200, 240] : [220, 277.18, 329.63, 440];
          base.forEach((frequency, index) => {
            const osc = ctx.createOscillator();
            osc.type = 'sine';
            osc.frequency.value = frequency;
            const oscGain = ctx.createGain();
            oscGain.gain.value = chosen === 'binaural' ? 0.4 : 0.12;
            if (chosen === 'binaural' && ctx.createStereoPanner) {
              const pan = ctx.createStereoPanner();
              pan.pan.value = index ? 1 : -1;
              osc.connect(oscGain);
              oscGain.connect(pan);
              pan.connect(gain);
            } else {
              osc.connect(oscGain);
              oscGain.connect(gain);
            }
            osc.start();
            nodes.push(osc);
          });
        }
        this.audio = nodes;
        this.gain = gain;
        this.setState({ soundOn: true, track: chosen });
      }
      stopAudio() {
        (this.audio || []).forEach((node) => { try { node.stop ? node.stop() : node.disconnect(); } catch (error) {} });
        this.audio = null;
        if (this.state.soundOn) this.setState({ soundOn: false });
      }
      chime() {
        try {
          const ctx = this.ensureCtx();
          ctx.resume();
          [660, 880].forEach((frequency, index) => {
            const osc = ctx.createOscillator(), gain = ctx.createGain(), time = ctx.currentTime + index * 0.18;
            osc.frequency.value = frequency;
            gain.gain.setValueAtTime(0, time);
            gain.gain.linearRampToValueAtTime(0.15, time + 0.02);
            gain.gain.exponentialRampToValueAtTime(0.001, time + 1.2);
            osc.connect(gain);
            gain.connect(ctx.destination);
            osc.start(time);
            osc.stop(time + 1.3);
          });
        } catch (error) {}
      }
      breath(elapsed) {
        const phases = [['Inhala', 5, 1], ['Exhala', 5, 0.45]];
        const cycle = phases.reduce((sum, phase) => sum + phase[1], 0);
        let cursor = elapsed % cycle, index = 0;
        while (cursor >= phases[index][1]) { cursor -= phases[index][1]; index++; }
        return { idx: index, label: phases[index][0], left: phases[index][1] - cursor, dur: phases[index][1], scale: phases[index][2], cycles: Math.floor(elapsed / cycle) };
      }
      breathTick() {
        const state = this.state;
        if (!(state.mode === 'break' && state.running && state.breathSound)) { this.stopBreathAudio(); return; }
        const ctx = this.ensureCtx();
        ctx.resume();
        if (!this.bNodes) {
          const source = ctx.createBufferSource();
          source.buffer = this.noiseBuf(ctx, false);
          source.loop = true;
          const filter = ctx.createBiquadFilter();
          filter.type = 'bandpass';
          filter.Q.value = 0.8;
          filter.frequency.value = 400;
          const low = ctx.createBiquadFilter();
          low.type = 'lowpass';
          low.frequency.value = 2500;
          const gain = ctx.createGain();
          gain.gain.value = 0.0001;
          source.connect(filter);
          filter.connect(low);
          low.connect(gain);
          gain.connect(ctx.destination);
          source.start();
          this.bNodes = { src: source, f: filter, g: gain };
          this.bKey = null;
        }
        const phase = this.breath(this.total() - this.rem());
        const limit = (state.p && state.p.breathCycles) || state.breathCycles || 6;
        if (phase.cycles >= limit) { this.stopBreathAudio(); return; }
        const key = phase.cycles + '-' + phase.idx;
        if (key === this.bKey) return;
        this.bKey = key;
        const { f, g } = this.bNodes;
        const now = ctx.currentTime;
        const end = now + phase.left;
        const vol = 0.22 * (state.volume / 100 + 0.3);
        g.gain.cancelScheduledValues(now);
        f.frequency.cancelScheduledValues(now);
        g.gain.setValueAtTime(Math.max(g.gain.value, 0.0001), now);
        f.frequency.setValueAtTime(f.frequency.value, now);
        if (phase.label === 'Inhala') {
          g.gain.linearRampToValueAtTime(vol, now + phase.left * 0.6);
          g.gain.linearRampToValueAtTime(vol * 0.5, end);
          f.frequency.linearRampToValueAtTime(1300, end);
        } else if (phase.label === 'Exhala') {
          g.gain.linearRampToValueAtTime(vol * 0.8, now + 0.4);
          g.gain.linearRampToValueAtTime(0.0001, end);
          f.frequency.linearRampToValueAtTime(350, end);
        } else g.gain.linearRampToValueAtTime(0.0001, now + 0.6);
      }
      stopBreathAudio() {
        if (!this.bNodes) return;
        try { this.bNodes.src.stop(); this.bNodes.g.disconnect(); } catch (error) {}
        this.bNodes = null;
        this.bKey = null;
      }

      prefs() {
        const state = this.state;
        const prefs = state.p || {};
        const set = (patch) => Ritmo.patch(this, patch);
        const toggle = (key, label, sub, disabled) => ({
          isToggle: true, label, sub: sub || '',
          tBg: prefs[key] ? '#32d74b' : 'rgba(255,255,255,.2)',
          tKnob: prefs[key] ? '15px' : '2px',
          toggle: () => { if (!disabled) set({ [key]: !prefs[key] }); },
          opacity: disabled ? 0.4 : 1,
        });
        const choice = (key, label, options, sub) => ({
          isChoice: true, label, sub: sub || '', stack: options.length > 4,
          options: options.map((option) => ({
            label: option,
            bg: prefs[key] === option ? 'rgba(255,255,255,.18)' : 'transparent',
            fg: prefs[key] === option ? '#fff' : 'rgba(235,235,245,.6)',
            select: () => set({ [key]: option }),
          })),
        });
        const stepper = (label, value, inc, dec, sub) => ({ isStepper: true, label, sub: sub || '', value, inc, dec });
        const linkSelect = (row) => ({
          isSelect: true, label: row.label, value: row.value,
          change: (event) => set({ [row.key]: event.target.value }),
        });
        const secretField = (field) => ({
          isSecret: true, stack: true, label: 'Clave de API',
          stored: !!field.stored, showInput: !field.stored, mask: field.mask || '',
          sub: field.stored ? 'Guardada. Pulsa Quitar para ingresar otra.' : 'Pega la clave y pulsa Guardar',
          value: field.draft || '', placeholder: field.placeholder || '',
          change: (event) => this.setState({ [field.draftKey]: event.target.value }),
          keyDown: (event) => { if (event.key === 'Enter' && !field.stored) field.save(); },
          save: field.save,
        });
        const off = !state.connected;
        const groups = {
          general: [
            { title: 'Acerca de', rows: [{ label: 'Ritmo ' + (state.appVersion || '0.1.0'), sub: 'Tu día, tus reuniones y tu enfoque en la barra de menú' }] },
            { title: 'Inicio', rows: [toggle('login', 'Abrir al iniciar sesión'), { isKey: true, label: 'Atajo para abrir el panel', sub: state.shortcutRecording ? 'Pulsa la combinación. Esc cancela.' : 'Pulsa el atajo para cambiarlo', value: state.shortcutRecording ? '…' : (prefs.shortcut || '⌥ ⌘ Espacio'), action: () => this.captureShortcut() }] },
            { title: 'Barra de menú', rows: [toggle('barTimer', 'Mostrar tiempo del pomodoro'), toggle('barMeeting', 'Mostrar cuenta regresiva de la próxima reunión'), choice('barMeetMin', 'Mostrar reuniones desde', ['15 min', '1 h', '12 h', '24 h', 'Hoy', 'Siempre'], 'Hoy muestra solo las que faltan hoy')] },
            { title: 'App', rows: [
              {
                isButton: true,
                label: state.updateAvailable ? 'Actualizar a ' + state.updateVersion : 'Actualizaciones',
                sub: state.updateInstalling ? 'Se descarga la versión nueva y Ritmo se reinicia' : state.updateAvailable ? (state.updateNotes || 'Hay una versión nueva en GitHub') : 'Busca en el repositorio público',
                value: state.updateInstalling || state.updateChecking ? '…' : state.updateAvailable ? 'Instalar' : 'Buscar',
                action: () => {
                  if (state.updateInstalling || state.updateChecking) return;
                  if (state.updateAvailable) Ritmo.installUpdate(this);
                  else Ritmo.checkUpdate(this, false);
                },
              },
              { isButton: true, label: 'Salir de la app', sub: '⌘Q', value: 'Salir', btnColor: '#ff6961', action: () => invoke('quit').catch(() => {}) },
            ] },
          ],
          cuenta: [
            { title: 'Factorial', note: 'La clave es de la empresa. Ritmo busca tu ficha con el correo de Google.', rows: [
              secretField({
                stored: state.factorialKeySet,
                mask: state.factorialKeyMask,
                draft: state.factorialDraft,
                placeholder: 'Clave de Factorial',
                draftKey: 'factorialDraft',
                save: () => Ritmo.setFactorialKey(this),
              }),
              ...(state.factorialKeySet ? [{ isButton: true, label: 'Quitar clave', sub: '', value: 'Quitar', btnColor: '#ff6961', action: () => Ritmo.clearFactorialKey(this) }] : []),
            ] },
            { title: 'ClickUp', note: 'Si hay varios espacios de trabajo, Ritmo usa el primero.', rows: [
              secretField({
                stored: state.clickupKeySet,
                mask: state.clickupKeyMask,
                draft: state.clickupDraft,
                placeholder: 'Clave de ClickUp',
                draftKey: 'clickupDraft',
                save: () => Ritmo.setClickupKey(this),
              }),
              ...(state.clickupKeySet ? [{ isButton: true, label: 'Quitar clave', sub: '', value: 'Quitar', btnColor: '#ff6961', action: () => Ritmo.clearClickupKey(this) }] : []),
            ] },
            { title: 'Servicios conectados', note: 'Los permisos se piden con tu cuenta de Google. Tasks abre de nuevo el consentimiento la primera vez.', rows: [
              toggle('svcCal', 'Google Calendar', 'Reuniones y enlaces', off),
              toggle('svcTasks', 'Google Tasks', 'Listas y tareas', off),
              toggle('svcGemini', 'Gemini', state.geminiKeySet ? 'Clave guardada en el llavero' : 'Falta la clave de API', off),
              toggle('svcFactorial', 'Factorial', state.factorialKeySet ? 'Cumpleaños, ausencias y fichaje' : 'Falta la clave de API'),
              toggle('svcClickup', 'ClickUp', state.clickupKeySet ? 'Tareas asignadas y tiempo de enfoque' : 'Falta la clave de API'),
            ] },
            { title: 'Chat', note: 'iframe abre Google Chat en su ventana. Enlace abre chat.google.com.', rows: [
              choice('chatUi', 'Cómo se abre', ['iframe', 'Enlace']),
            ] },
            { title: 'Clave de Gemini', rows: state.geminiKeySet ? [
              secretField({ stored: true, mask: state.geminiKeyMask, draft: '', placeholder: '', draftKey: 'geminiDraft', save: () => {} }),
              { isButton: true, label: 'Quitar clave', sub: '', value: 'Quitar', btnColor: '#ff6961', action: () => Ritmo.clearGeminiKey(this) },
            ] : [
              { isButton: true, label: 'Guardar clave de API', sub: 'No se incluye en la app', value: 'Guardar', btnColor: '#f5f5f7', action: () => Ritmo.setGeminiKey(this) },
            ] },
            { title: 'Otras cuentas', rows: [{ isButton: true, label: 'Agregar otra cuenta de Google', sub: 'Por ahora Ritmo usa una sola cuenta', value: 'Agregar…', btnColor: '#f5f5f7', action: () => toast(this, 'Por ahora Ritmo usa una sola cuenta de Google.') }] },
          ],
          calendario: [
            { title: 'Calendarios visibles', rows: (state.calendars || []).length ? state.calendars.map((calendar) => ({
              ...toggle('cal-' + calendar.id, calendar.label, calendar.sub, off),
              dot: calendar.color,
              tBg: calendar.selected ? '#32d74b' : 'rgba(255,255,255,.2)',
              tKnob: calendar.selected ? '15px' : '2px',
              toggle: () => !off && Ritmo.calendar(this, calendar.id, !calendar.selected),
            })) : [{ label: 'Sin calendarios', sub: state.connected ? 'Aparecen al sincronizar la cuenta' : 'Conecta tu cuenta para verlos' }] },
            { title: 'Abrir enlaces', note: 'Cada servicio puede abrirse en su app o en el navegador que elijas. Teléfono y salas usan el sistema.', rows: (prefs.linkRows || []).map(linkSelect) },
            { title: 'Detección de reuniones', note: 'Se buscan enlaces en la ubicación y, si lo activas, en la descripción. Compatible con Meet, Hangouts, Zoom, Teams, Webex, Slack, Jitsi, Whereby, GoTo, Chime, Discord, FaceTime, teléfono y salas.', rows: [toggle('detectDesc', 'Buscar en descripción y ubicación'), toggle('detectNotes', 'Buscar enlaces en notas')] },
            { title: 'Reuniones', rows: [choice('range', 'Mostrar próximos', ['3 días', '7 días', '14 días']), choice('notify', 'Avisar antes de empezar', ['1 min', '5 min', '10 min']), toggle('autoJoin', 'Abrir el enlace al empezar'), toggle('hideDeclined', 'Ocultar reuniones rechazadas'), { isButton: true, label: 'Probar aviso', sub: 'Envía una alerta de ejemplo. Unirse abre Meet.', value: 'Enviar', btnColor: '#f5f5f7', action: () => Ritmo.previewAlert(this) }] },
          ],
          pomodoro: [
            { title: 'Duración', rows: [
              stepper('Enfoque', prefs.focusMin + ' min', () => set({ focusMin: Math.min(90, prefs.focusMin + 5) }), () => set({ focusMin: Math.max(5, prefs.focusMin - 5) })),
              stepper('Descanso corto', prefs.breakMin + ' min', () => set({ breakMin: Math.min(30, prefs.breakMin + 1) }), () => set({ breakMin: Math.max(1, prefs.breakMin - 1) })),
              stepper('Descanso largo', prefs.longBreak + ' min', () => set({ longBreak: Math.min(45, prefs.longBreak + 5) }), () => set({ longBreak: Math.max(5, prefs.longBreak - 5) }), 'Después de 4 sesiones'),
            ] },
            { title: 'Comportamiento', rows: [toggle('autoBreak', 'Iniciar descansos automáticamente'), toggle('pauseMeet', 'Pausar al empezar una reunión')] },
            { title: 'Respiración', note: 'El ejercicio se cierra al terminar los ciclos. El descanso del pomodoro sigue.', rows: [
              toggle('breathSound', 'Sonido al respirar', 'Se oye durante el descanso'),
              stepper('Ciclos', String(prefs.breathCycles || 6), () => set({ breathCycles: Math.min(12, (prefs.breathCycles || 6) + 1) }), () => set({ breathCycles: Math.max(1, (prefs.breathCycles || 6) - 1) }), 'Después desaparece'),
            ] },
          ],
          gemini: [
            { title: 'Asistente', rows: [toggle('aiDaily', 'Resumen del día al abrir', '', off || !prefs.svcGemini), choice('aiTime', 'Preparar resumen', ['8:00', '9:00', 'Al abrir']), toggle('aiSuggest', 'Sugerir pomodoros en huecos libres')] },
            { title: 'Privacidad', note: 'La clave de Gemini se queda en el llavero de este equipo. El resumen sin clave usa tu agenda y tus tareas, sin inventar reuniones.', rows: [toggle('aiNotes', 'Permitir leer notas locales'), { label: 'Correo', sub: 'Ritmo no tiene un buzón conectado.' }] },
          ],
        };
        return (groups[state.prefsTab] || []).map((group) => ({
          ...group,
          note: group.note || '',
          rows: group.rows.map((row, index) => ({
            dot: '', sub: '', opacity: 1, isToggle: false, isStepper: false, isChoice: false, isSelect: false, isKey: false, isButton: false, isSecret: false, stored: false, showInput: false, mask: '', action: () => {},
            border: index ? '.5px solid rgba(255,255,255,.08)' : 'none',
            direction: row.stack ? 'column' : 'row',
            align: row.stack ? 'stretch' : 'center',
            choiceAlign: row.stack ? 'flex-end' : 'auto',
            ...row,
          })),
        }));
      }

      renderVals() {
        const state = this.state;
        const taskMark = (task, accent) => {
          const phase = state.taskPulse && state.taskPulse.id === task.id ? state.taskPulse.phase : '';
          const done = !!task.done || phase === 'done';
          const loading = phase === 'loading';
          const idle = 'rgba(235,235,245,.35)';
          return {
            edge: loading ? '1.6px solid rgba(255,255,255,.22)' : '1.5px solid ' + (done ? accent : idle),
            edgeTop: loading || done ? accent : idle,
            fill: done && !loading ? accent : 'transparent',
            mark: loading || !done ? '' : '✓',
            motion: loading ? 'ritmo-spin .7s linear infinite' : phase === 'done' ? 'ritmo-check .32s ease' : phase === 'error' ? 'ritmo-shake .42s ease' : 'none',
          };
        };
        const remaining = this.rem();
        const total = this.total();
        const modeColor = state.mode === 'focus' ? '#ff453a' : '#32d74b';
        const timeText = String(Math.floor(remaining / 60)).padStart(2, '0') + ':' + String(remaining % 60).padStart(2, '0');
        const focusTask = [...(state.taskItems || []), ...(state.clickupItems || [])].find((task) => task.id === state.focusTaskId);
        const join = (url, meta) => Ritmo.openMeeting(this, url, meta);
        const eventRow = (event) => ({
          ...event,
          opacity: event.past ? 0.4 : 1,
          anchor: event.live ? 'relative' : 'static',
          inset: event.live ? '3px 10px' : '0',
          box: event.live ? '6px 8px' : '6px 18px',
          edge: event.live ? '1.5px solid var(--ritmo-live, #ff453a)' : 'none',
          curve: event.live ? '12px' : '0',
          pulse: event.live ? 'ritmo-live 3s ease-in-out infinite' : 'none',
          joinTitle: event.joinUrl,
          join: () => join(event.joinUrl, { color: event.provColor, abbr: event.provAbbr }),
        });
        const phase = this.breath(total - remaining);
        const breathMax = (state.p && state.p.breathCycles) || state.breathCycles || 6;
        const breathing = state.mode === 'break' && phase.cycles < breathMax;
        if (breathing) this._breathOnScreen = true;
        const exiting = !breathing && !!this._breathOnScreen;
        this._wantBreathExit = exiting;
        const live = breathing && state.running;
        return {
          isBreak: breathing || exiting,
          breathMin: exiting ? '0' : '188px',
          breathOverflow: exiting ? 'hidden' : 'visible',
          breathPointer: exiting ? 'none' : 'auto',
          breathLeave: exiting ? 'ritmo-breath-out .6s ease forwards' : 'none',
          breathOrb: (() => {
            const open = live && phase.scale > 0.9;
            const duration = live ? phase.dur : 0.8;
            const ease = 'cubic-bezier(.45,.05,.55,.95)';
            const cols = ['rgba(64,224,160,.5)', 'rgba(52,199,190,.5)', 'rgba(120,235,150,.5)'];
            const petals = Array.from({ length: 8 }, (_, index) => React.createElement('div', {
              key: index,
              style: {
                position: 'absolute', left: '50%', top: '50%', width: 74, height: 74, marginLeft: -37, marginTop: -37, borderRadius: '50%',
                background: 'radial-gradient(circle at 50% 30%, ' + cols[index % 3].replace('.5)', '.75)') + ', ' + cols[index % 3] + ')',
                mixBlendMode: 'screen',
                transform: 'rotate(' + (index * 45) + 'deg) translateY(' + (open ? -40 : -6) + 'px)',
                transition: 'transform ' + duration + 's ' + ease,
              },
            }));
            return React.createElement('div', {
              key: 'bloom',
              style: { position: 'relative', width: 170, height: 170, transform: 'rotate(' + (open ? 90 : 0) + 'deg) scale(' + (open ? 1 : 0.62) + ')', transition: 'transform ' + duration + 's ' + ease, filter: 'drop-shadow(0 0 18px rgba(64,224,160,.3))' },
            }, petals);
          })(),
          breathRunFg: state.running ? '#b7f7d4' : 'rgba(235,235,245,.5)',
          breathRunBorder: state.running ? 'rgba(183,247,212,.7)' : 'rgba(255,255,255,.2)',
          breathHint: (() => {
            const left = Math.max(1, breathMax - phase.cycles);
            return phase.label + ' · ' + (left === 1 ? 'falta 1' : 'faltan ' + left);
          })(),
          breathCta: state.running ? 'Pausar' : 'Empezar',
          skipBreak: () => Ritmo.pomodoro(this, 'skip'),
          panelOpen: true,
          notApp: !state.isApp,
          togglePanel: () => {},
          showBarTimer: state.showBarTimer,
          showBarMeeting: state.showBarMeeting,
          showBarDivider: state.showBarTimer && state.showBarMeeting,
          showBarIconOnly: !state.showBarTimer && !state.showBarMeeting,
          barMeetingLabel: state.barMeetingLabel,
          menuClock: state.menuClock,
          weekdayLabel: state.weekdayLabel,
          dateLong: state.dateLong,
          prefsOpen: state.prefsOpen,
          prefsBtnBg: state.prefsOpen ? 'rgba(255,255,255,.16)' : 'transparent',
          openPrefs: () => {
            const next = !state.prefsOpen;
            this.setState({ prefsOpen: next, musicOpen: false, taskMenuOpen: false });
            if (!window.__TAURI__) return;
            invoke(next ? 'show_settings_window' : 'hide_settings_window').catch((error) => {
              this.setState({ prefsOpen: !next });
              toast(this, error);
            });
          },
          closePrefs: () => {
            this.setState({ prefsOpen: false });
            if (window.__TAURI__) invoke('hide_settings_window').catch(() => {});
          },
          prefTabs: [['general', 'General', '⚙︎', '#8e8e93'], ['cuenta', 'Cuenta', 'G', '#0a84ff'], ['calendario', 'Calendario', '▦', '#ff453a'], ['pomodoro', 'Pomodoro', '◎', '#ff9f0a'], ['gemini', 'Gemini', '✦', '#bf5af2']].map(([key, label, icon, color]) => ({
            label, icon, iconBg: color,
            bg: state.prefsTab === key ? '#151515' : 'transparent',
            fg: state.prefsTab === key ? '#f5f5f7' : 'rgba(235,235,245,.6)',
            select: () => this.setState({ prefsTab: key }),
          })),
          prefsTitle: { general: 'General', cuenta: 'Cuenta', calendario: 'Calendario', pomodoro: 'Pomodoro', gemini: 'Gemini' }[state.prefsTab],
          isAccountTab: state.prefsTab === 'cuenta',
          accountName: state.accountName,
          accountEmail: state.accountEmail,
          accountInitials: state.accountInitials,
          accountStatus: state.accountStatus,
          accountStatusColor: state.accountStatusColor,
          signingIn: !!state.signingIn,
          connectLabel: state.signingIn ? 'Conectando…' : (state.connected ? 'Cerrar sesión' : 'Iniciar sesión con Google'),
          connectBg: state.connected ? 'rgba(255,255,255,.14)' : '#0a84ff',
          toggleConnect: () => {
            if (state.signingIn) return;
            if (state.connected) Ritmo.signOut(this);
            else Ritmo.signIn(this);
          },
          prefGroups: this.prefs(),
          timeText, modeColor,
          modeName: state.mode === 'focus' ? (state.running ? 'Enfoque · ' + state.session + ' de 4' : 'En pausa') : (state.p && state.p.longBreak && state.session >= 4 ? 'Descanso largo' : 'Descanso'),
          ringOffset: (163.36 * (remaining / total)).toFixed(2),
          runIcon: state.running ? '❚❚' : '▶',
          running: state.running,
          isPaused: !state.running,
          toggleRun: () => Ritmo.pomodoro(this, 'toggle'),
          modes: [['focus', 'Enfoque', '◎'], ['break', 'Descanso', '☾']].map(([key, label, icon]) => ({
            label, icon,
            bg: state.mode === key ? 'rgba(255,255,255,.16)' : 'transparent',
            fg: state.mode === key ? (key === 'focus' ? '#ff453a' : '#32d74b') : 'rgba(235,235,245,.5)',
            select: () => Ritmo.pomodoro(this, key),
          })),
          taskMenuOpen: state.taskMenuOpen,
          toggleTaskMenu: () => this.setState({ taskMenuOpen: !state.taskMenuOpen }),
          focusTaskLabel: focusTask ? focusTask.title : 'Elegir tarea',
          focusTaskColor: focusTask ? 'rgba(245,245,247,.85)' : '#0a84ff',
          focusOptions: [{ id: null, title: 'Sin tarea' }, ...(state.taskItems || []).filter((task) => !task.done), ...(state.clickupItems || [])].map((task) => ({
            title: task.title,
            check: state.focusTaskId === task.id ? '✓' : '',
            count: task.id && state.completed[task.id] ? state.completed[task.id] + ' ◎' : '',
            select: () => { this.setState({ taskMenuOpen: false }); Ritmo.pomodoro(this, 'task', task.id); },
          })),
          aiOpen: state.aiOpen,
          toggleAi: () => {
            const next = !state.aiOpen;
            this.setState({ aiOpen: next });
            if (!window.__TAURI__) return;
            invoke(next ? 'show_gemini_window' : 'hide_gemini_window').catch((error) => toast(this, error));
          },
          chatOpen: state.chatOpen,
          toggleChat: () => {
            const mode = (state.p && state.p.chatUi) || 'iframe';
            if (mode === 'Enlace') {
              this.setState({ chatOpen: false });
              Ritmo.launchChat(this);
              return;
            }
            const next = !state.chatOpen;
            this.setState({ chatOpen: next });
          },
          chatBtnTitle: (state.p && state.p.chatUi) === 'Enlace' ? 'Abrir Google Chat' : 'Google Chat',
          chatBtnBg: state.chatOpen ? 'rgba(255,255,255,.16)' : 'rgba(255,255,255,.08)',
          aiBtnBg: state.aiOpen ? 'rgba(255,255,255,.16)' : 'rgba(255,255,255,.08)',
          aiLoading: !!state.aiLoading,
          aiReady: !state.aiLoading,
          aiAsking: !!state.aiAsking,
          aiSummary: state.aiSummary || 'El resumen usa tu agenda real cuando Gemini tiene clave. Sin clave, Ritmo arma un resumen con tus eventos y tareas.',
          refreshAi: () => Ritmo.summary(this),
          aiDraft: state.aiDraft,
          setAiDraft: (event) => this.setState({ aiDraft: event.target.value }),
          aiKey: (event) => { if (event.key === 'Enter') this.sendAi(); },
          sendAi: () => { const question = (state.aiDraft || '').trim(); if (question) Ritmo.ask(this, question); },
          sendBg: (state.aiDraft || '').trim() ? '#0a84ff' : 'rgba(255,255,255,.15)',
          chat: (state.aiChat || []).map((entry) => ({ ...entry, isUser: entry.role === 'user', isAi: entry.role === 'ai' })),
          actions: (state.suggestions || []).map((item) => ({
            ...item,
            when: item.whenLabel,
            run: () => {
              if (item.action === 'open-notes') this.setState({ tab: 'notas' });
              else Ritmo.pomodoro(this, 'start', item.targetId);
            },
          })),
          segPos: (['hoy', 'prox', 'notas'].indexOf(state.tab) * 33.333).toFixed(3) + '%',
          tabs: [['hoy', 'Hoy'], ['prox', 'Próximos'], ['notas', 'Notas']].map(([key, label]) => ({
            label, fg: state.tab === key ? '#f5f5f7' : 'rgba(235,235,245,.6)', select: () => this.setState({ tab: key }),
          })),
          booting: !!state.booting && !(state.agendaToday || []).some((row) => row.isEvent),
          eventsLoading: (!!state.eventsLoading || !!state.booting) && !(state.agendaToday || []).some((row) => row.isEvent),
          tasksLoading: (!!state.tasksLoading || !!state.booting) && !(state.taskItems || []).length,
          isHoy: state.tab === 'hoy',
          isProx: state.tab === 'prox',
          isNotas: state.tab === 'notas',
          nowMark: state.nowMark || '',
          today: (() => {
            const rows = (state.agendaToday || []).map(eventRow);
            const nowAt = rows.findIndex((row) => row.isNow);
            const stillAhead = nowAt >= 0 && rows.slice(nowAt + 1).some((row) => row.isEvent && row.time !== 'Día');
            if (nowAt >= 0 && !stillAhead && !state.eventsLoading) {
              rows.splice(nowAt + 1, 0, { isDone: true, isNow: false, isEvent: false });
            }
            return rows;
          })(),
          days: (state.agendaDays || []).map((day) => ({ ...day, events: (day.events || []).map(eventRow) })),
          notes: (state.noteItems || []).map((note) => ({ ...note, join: () => join(note.joinUrl, { color: note.provColor, abbr: note.provAbbr }) })),
          addNote: () => {
            const title = window.prompt('Título de la nota');
            if (title == null) return;
            const body = window.prompt('Texto de la nota', '') || '';
            Ritmo.addNote(this, title, body);
          },
          nextTitle: state.nextTitle,
          nextMeta: state.nextMeta,
          nextColor: state.nextColor,
          nextAbbr: state.nextAbbr,
          nextJoinTitle: state.nextJoinUrl,
          joinNext: () => join(state.nextJoinUrl, { color: state.nextColor, abbr: state.nextAbbr }),
          hasToast: !!state.toast,
          toastText: state.toast ? state.toast.text : '',
          toastColor: state.toast ? state.toast.color : 'transparent',
          toastAbbr: state.toast ? state.toast.abbr : '',
          pendingCount: (state.taskItems || []).filter((task) => !task.done).length,
          hasTaskHistory: (state.taskItems || []).some((task) => task.done && !task.doneToday),
          historyLabel: state.taskHistory ? 'Ocultar' : 'Historial',
          toggleHistory: () => this.setState({ taskHistory: !state.taskHistory }),
          taskAdding: !!state.taskAdding,
          taskReady: !state.taskAdding,
          draft: state.draft,
          setDraft: (event) => this.setState({ draft: event.target.value }),
          addKey: (event) => {
            if (event.key !== 'Enter' || state.taskAdding) return;
            const title = (state.draft || '').trim();
            if (title) Ritmo.addTask(this, title);
          },
          showClickup: !!(state.p && state.p.svcClickup),
          clickupCount: (state.clickupItems || []).length,
          clickupLoading: !!state.clickupLoading,
          clickupEmpty: !!(state.p && state.p.svcClickup) && !state.clickupLoading && !(state.clickupItems || []).length,
          clickupGroups: (state.clickupGroups || []).map((group) => ({
            label: group.label,
            tasks: (group.tasks || []).map((task) => ({
              ...task,
              ...taskMark(task, '#ff5a36'),
              hasDue: !!task.due,
              dueColor: task.late ? '#ff453a' : 'rgba(235,235,245,.5)',
              focusColor: state.focusTaskId === task.id ? '#ff453a' : 'rgba(235,235,245,.3)',
              toggle: () => Ritmo.toggleTask(this, task.id),
              focus: () => Ritmo.pomodoro(this, 'start', task.id),
            })),
          })),
          tasks: (state.taskItems || []).filter((task) => !task.done || state.taskHistory || task.doneToday || (state.taskPulse && state.taskPulse.id === task.id)).map((task) => ({
            ...task,
            ...taskMark(task, '#0a84ff'),
            enter: state.freshTaskId === task.id ? 'ritmo-task-in .5s ease' : 'none',
            color: (task.done || (state.taskPulse && state.taskPulse.id === task.id && state.taskPulse.phase === 'done')) ? 'rgba(235,235,245,.4)' : '#f5f5f7',
            deco: (task.done || (state.taskPulse && state.taskPulse.id === task.id && state.taskPulse.phase === 'done')) ? 'line-through' : 'none',
            due: task.done ? '' : task.due,
            dueColor: task.late ? '#ff453a' : 'rgba(235,235,245,.5)',
            focusColor: state.focusTaskId === task.id ? '#ff453a' : 'rgba(235,235,245,.3)',
            toggle: () => Ritmo.toggleTask(this, task.id),
            focus: () => Ritmo.pomodoro(this, 'start', task.id),
          })),
        };
      }
    };
  };

})();
