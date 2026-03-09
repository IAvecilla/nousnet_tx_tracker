// Psyche Transaction Tracker - Frontend Application

class TransactionTracker {
    constructor() {
        this.transactions = [];
        this.ws = null;
        this.isPaused = false;
        this.autoScroll = true;
        this.filters = {
            run_id: '',
            signer: '',
            instruction_type: '',
            program_name: ''
        };

        this.init();
    }

    init() {
        this.bindElements();
        this.bindEvents();
        this.loadInitialData();
        this.connectWebSocket();
    }

    bindElements() {
        this.elements = {
            connectionStatus: document.getElementById('connection-status'),
            statusDot: document.querySelector('.status-dot'),
            statusText: document.querySelector('.status-text'),
            statTotal: document.getElementById('stat-total'),
            statSigners: document.getElementById('stat-signers'),
            statRuns: document.getElementById('stat-runs'),
            statLatest: document.getElementById('stat-latest'),
            filterRunId: document.getElementById('filter-run-id'),
            filterSigner: document.getElementById('filter-signer'),
            filterType: document.getElementById('filter-type'),
            filterProgram: document.getElementById('filter-program'),
            btnApplyFilters: document.getElementById('btn-apply-filters'),
            btnClearFilters: document.getElementById('btn-clear-filters'),
            btnExport: document.getElementById('btn-export'),
            autoScrollCheckbox: document.getElementById('auto-scroll'),
            btnPause: document.getElementById('btn-pause'),
            transactionsList: document.getElementById('transactions-list'),
            modal: document.getElementById('tx-detail-modal'),
            modalClose: document.getElementById('modal-close'),
            modalContent: document.getElementById('tx-detail-content')
        };
    }

    bindEvents() {
        this.elements.btnApplyFilters.addEventListener('click', () => this.applyFilters());
        this.elements.btnClearFilters.addEventListener('click', () => this.clearFilters());
        this.elements.btnExport.addEventListener('click', () => this.exportTransactions());
        this.elements.btnPause.addEventListener('click', () => this.togglePause());
        this.elements.autoScrollCheckbox.addEventListener('change', (e) => {
            this.autoScroll = e.target.checked;
        });
        this.elements.modalClose.addEventListener('click', () => this.closeModal());
        this.elements.modal.addEventListener('click', (e) => {
            if (e.target === this.elements.modal) this.closeModal();
        });

        // Enter key for filters
        [this.elements.filterRunId, this.elements.filterSigner].forEach(el => {
            el.addEventListener('keyup', (e) => {
                if (e.key === 'Enter') this.applyFilters();
            });
        });
    }

    async loadInitialData() {
        try {
            // Load stats
            const statsResponse = await fetch('/api/stats');
            const stats = await statsResponse.json();
            this.updateStats(stats);

            // Load initial transactions
            const txResponse = await fetch('/api/transactions?limit=100');
            const transactions = await txResponse.json();
            this.transactions = transactions;
            this.renderTransactions();
        } catch (error) {
            console.error('Failed to load initial data:', error);
            this.elements.transactionsList.innerHTML = `
                <div class="loading">Failed to load data. Is the server running?</div>
            `;
        }
    }

    connectWebSocket() {
        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const wsUrl = `${protocol}//${window.location.host}/ws`;

        this.ws = new WebSocket(wsUrl);

        this.ws.onopen = () => {
            this.updateConnectionStatus(true);
            console.log('WebSocket connected');
        };

        this.ws.onclose = () => {
            this.updateConnectionStatus(false);
            console.log('WebSocket disconnected, reconnecting...');
            setTimeout(() => this.connectWebSocket(), 3000);
        };

        this.ws.onerror = (error) => {
            console.error('WebSocket error:', error);
        };

        this.ws.onmessage = (event) => {
            try {
                const message = JSON.parse(event.data);
                this.handleWebSocketMessage(message);
            } catch (error) {
                console.error('Failed to parse WebSocket message:', error);
            }
        };
    }

    handleWebSocketMessage(message) {
        switch (message.type) {
            case 'NewTransaction':
                if (!this.isPaused) {
                    this.addTransaction(message.data);
                }
                break;
            case 'StatsUpdate':
                this.updateStats(message.data);
                break;
            case 'Connected':
                console.log('Server:', message.data.message);
                break;
            case 'Error':
                console.error('Server error:', message.data.message);
                break;
        }
    }

    updateConnectionStatus(connected) {
        this.elements.statusDot.className = `status-dot ${connected ? 'connected' : 'disconnected'}`;
        this.elements.statusText.textContent = connected ? 'Connected' : 'Disconnected';
    }

    updateStats(stats) {
        this.elements.statTotal.textContent = stats.total_count.toLocaleString();
        this.elements.statSigners.textContent = stats.unique_signers.toLocaleString();
        this.elements.statRuns.textContent = stats.run_ids.length;

        if (stats.latest_time) {
            const date = new Date(stats.latest_time * 1000);
            this.elements.statLatest.textContent = date.toLocaleTimeString();
        }
    }

    addTransaction(tx) {
        // Check if transaction matches current filters
        if (!this.matchesFilters(tx)) return;

        // Add to beginning of array
        this.transactions.unshift(tx);

        // Limit stored transactions
        if (this.transactions.length > 1000) {
            this.transactions.pop();
        }

        // Add to DOM
        const txElement = this.createTransactionElement(tx, true);
        const list = this.elements.transactionsList;

        // Remove loading message if present
        const loading = list.querySelector('.loading');
        if (loading) loading.remove();

        list.insertBefore(txElement, list.firstChild);

        // Auto-scroll
        if (this.autoScroll) {
            list.scrollTop = 0;
        }

        // Update stats
        this.refreshStats();
    }

    matchesFilters(tx) {
        if (this.filters.run_id && tx.run_id !== this.filters.run_id) return false;
        if (this.filters.signer && tx.signer !== this.filters.signer) return false;
        if (this.filters.instruction_type && tx.instruction_type !== this.filters.instruction_type) return false;
        if (this.filters.program_name && tx.program_name !== this.filters.program_name) return false;
        return true;
    }

    renderTransactions() {
        const list = this.elements.transactionsList;
        list.innerHTML = '';

        if (this.transactions.length === 0) {
            list.innerHTML = '<div class="loading">No transactions found</div>';
            return;
        }

        const filtered = this.transactions.filter(tx => this.matchesFilters(tx));

        if (filtered.length === 0) {
            list.innerHTML = '<div class="loading">No transactions match the current filters</div>';
            return;
        }

        filtered.forEach(tx => {
            list.appendChild(this.createTransactionElement(tx));
        });
    }

    createTransactionElement(tx, isNew = false) {
        const div = document.createElement('div');
        div.className = `tx-item ${isNew ? 'new' : ''}`;
        div.onclick = () => this.showTransactionDetail(tx);

        const time = tx.block_time
            ? new Date(tx.block_time * 1000).toLocaleString()
            : `Slot ${tx.slot}`;

        const shortSig = tx.signature.substring(0, 8) + '...' + tx.signature.substring(tx.signature.length - 8);
        const shortSigner = tx.signer.substring(0, 8) + '...' + tx.signer.substring(tx.signer.length - 4);

        div.innerHTML = `
            <div class="tx-type ${tx.program_name}">${tx.instruction_type}</div>
            <div class="tx-signature" title="${tx.signature}">${shortSig}</div>
            <div class="tx-signer" title="${tx.signer}">${shortSigner}</div>
            <div class="tx-time">${time}</div>
            <div class="tx-status">
                <span class="${tx.success ? 'success' : 'failed'}">${tx.success ? 'OK' : 'FAIL'}</span>
            </div>
        `;

        return div;
    }

    showTransactionDetail(tx) {
        const solscanUrl = `https://solscan.io/tx/${tx.signature}?cluster=devnet`;

        let detailHtml = `
            <div class="detail-row">
                <div class="detail-label">Signature</div>
                <div class="detail-value">
                    <a href="${solscanUrl}" target="_blank" rel="noopener">${tx.signature}</a>
                </div>
            </div>
            <div class="detail-row">
                <div class="detail-label">Slot</div>
                <div class="detail-value">${tx.slot}</div>
            </div>
            <div class="detail-row">
                <div class="detail-label">Block Time</div>
                <div class="detail-value">${tx.block_time ? new Date(tx.block_time * 1000).toLocaleString() : 'N/A'}</div>
            </div>
            <div class="detail-row">
                <div class="detail-label">Signer</div>
                <div class="detail-value">${tx.signer}</div>
            </div>
            <div class="detail-row">
                <div class="detail-label">Program</div>
                <div class="detail-value">${tx.program_name} (${tx.program_id})</div>
            </div>
            <div class="detail-row">
                <div class="detail-label">Instruction</div>
                <div class="detail-value">${tx.instruction_type}</div>
            </div>
            <div class="detail-row">
                <div class="detail-label">Status</div>
                <div class="detail-value">${tx.success ? 'Success' : 'Failed'}</div>
            </div>
        `;

        if (tx.run_id) {
            detailHtml += `
                <div class="detail-row">
                    <div class="detail-label">Run ID</div>
                    <div class="detail-value">${tx.run_id}</div>
                </div>
            `;
        }

        if (tx.client_pubkey) {
            detailHtml += `
                <div class="detail-row">
                    <div class="detail-label">Client</div>
                    <div class="detail-value">${tx.client_pubkey}</div>
                </div>
            `;
        }

        if (tx.logs && tx.logs.length > 0) {
            detailHtml += `
                <div class="logs-container">
                    <strong>Program Logs:</strong>
                    ${tx.logs.map(log => `<p>${this.escapeHtml(log)}</p>`).join('')}
                </div>
            `;
        }

        this.elements.modalContent.innerHTML = detailHtml;
        this.elements.modal.classList.add('active');
    }

    closeModal() {
        this.elements.modal.classList.remove('active');
    }

    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }

    async applyFilters() {
        this.filters = {
            run_id: this.elements.filterRunId.value.trim(),
            signer: this.elements.filterSigner.value.trim(),
            instruction_type: this.elements.filterType.value,
            program_name: this.elements.filterProgram.value
        };

        // Build query string
        const params = new URLSearchParams();
        if (this.filters.run_id) params.append('run_id', this.filters.run_id);
        if (this.filters.signer) params.append('signer', this.filters.signer);
        if (this.filters.instruction_type) params.append('instruction_type', this.filters.instruction_type);
        if (this.filters.program_name) params.append('program_name', this.filters.program_name);
        params.append('limit', '500');

        try {
            const response = await fetch(`/api/transactions?${params.toString()}`);
            this.transactions = await response.json();
            this.renderTransactions();
        } catch (error) {
            console.error('Failed to apply filters:', error);
        }
    }

    clearFilters() {
        this.elements.filterRunId.value = '';
        this.elements.filterSigner.value = '';
        this.elements.filterType.value = '';
        this.elements.filterProgram.value = '';

        this.filters = {
            run_id: '',
            signer: '',
            instruction_type: '',
            program_name: ''
        };

        this.loadInitialData();
    }

    togglePause() {
        this.isPaused = !this.isPaused;
        this.elements.btnPause.textContent = this.isPaused ? 'Resume' : 'Pause';
    }

    async refreshStats() {
        try {
            const params = new URLSearchParams();
            if (this.filters.run_id) params.append('run_id', this.filters.run_id);

            const response = await fetch(`/api/stats?${params.toString()}`);
            const stats = await response.json();
            this.updateStats(stats);
        } catch (error) {
            console.error('Failed to refresh stats:', error);
        }
    }

    exportTransactions() {
        const data = JSON.stringify(this.transactions, null, 2);
        const blob = new Blob([data], { type: 'application/json' });
        const url = URL.createObjectURL(blob);

        const a = document.createElement('a');
        a.href = url;
        a.download = `psyche-transactions-${Date.now()}.json`;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(url);
    }
}

// Initialize on DOM ready
document.addEventListener('DOMContentLoaded', () => {
    window.tracker = new TransactionTracker();
});
