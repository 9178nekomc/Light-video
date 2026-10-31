// ==================== 全局变量 ====================
let searchTimeout;

// ==================== 页面初始化 ====================
document.addEventListener('DOMContentLoaded', function() {
    // 初始化下拉菜单
    initDropdown();
    
    // 更新未读通知数量
    updateUnreadNotifications();
    updateUnreadMessages();
    
    // 自动滚动到聊天底部
    const chatMessages = document.getElementById('chat-messages');
    if (chatMessages) {
        chatMessages.scrollTop = chatMessages.scrollHeight;
    }
    
    // 自动关闭提示消息
    const alerts = document.querySelectorAll('.alert');
    alerts.forEach(alert => {
        setTimeout(() => {
            alert.style.opacity = '0';
            setTimeout(() => {
                alert.style.display = 'none';
            }, 300);
        }, 3000);
    });
    
    // 视频卡片点击跳转
    document.querySelectorAll('.video-card').forEach(card => {
        card.addEventListener('click', function(e) {
            // 如果点击的是作者链接，不跳转
            if (e.target.classList.contains('video-author') || 
                e.target.classList.contains('friend-action-btn') ||
                e.target.closest('.friend-action-btn')) {
                return;
            }
            const videoId = this.dataset.videoId;
            if (videoId) {
                window.location.href = `/video/${videoId}`;
            }
        });
    });
    
    // 点击消息输入框外的区域关闭模态框
    window.onclick = function(event) {
        const modal = document.getElementById('share-modal');
        if (event.target == modal) {
            closeShareModal();
        }
    };
});

// ==================== 工具函数 ====================

// 格式化日期
function formatDate(dateString) {
    const date = new Date(dateString);
    const now = new Date();
    const diffTime = Math.abs(now - date);
    const diffDays = Math.floor(diffTime / (1000 * 60 * 60 * 24));
    const diffHours = Math.floor(diffTime / (1000 * 60 * 60));
    const diffMinutes = Math.floor(diffTime / (1000 * 60));
    
    if (diffMinutes < 1) {
        return '刚刚';
    } else if (diffMinutes < 60) {
        return diffMinutes + '分钟前';
    } else if (diffHours < 24) {
        return diffHours + '小时前';
    } else if (diffDays < 30) {
        return diffDays + '天前';
    } else {
        return date.toLocaleDateString('zh-CN');
    }
}

// HTML转义
function escapeHtml(unsafe) {
    return unsafe
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;")
        .replace(/'/g, "&#039;");
}

// ==================== 导航下拉菜单 ====================

// 初始化下拉菜单
function initDropdown() {
    const dropdown = document.getElementById('userDropdown');
    if (!dropdown) return;
    
    // 点击其他地方关闭下拉菜单
    document.addEventListener('click', function(event) {
        const dropdown = document.getElementById('userDropdown');
        const dropdownMenu = document.getElementById('dropdownMenu');
        
        if (dropdown && !dropdown.contains(event.target)) {
            dropdownMenu.classList.remove('show');
            const arrow = dropdown.querySelector('.dropdown-arrow');
            if (arrow) {
                arrow.style.transform = 'rotate(0deg)';
            }
        }
    });
}

// 切换下拉菜单显示
function toggleDropdown(event) {
    event.stopPropagation();
    const dropdownMenu = document.getElementById('dropdownMenu');
    const arrow = document.querySelector('.dropdown-arrow');
    
    dropdownMenu.classList.toggle('show');
    
    if (arrow) {
        if (dropdownMenu.classList.contains('show')) {
            arrow.style.transform = 'rotate(180deg)';
        } else {
            arrow.style.transform = 'rotate(0deg)';
        }
    }
}

// 按ESC键关闭下拉菜单
document.addEventListener('keydown', function(event) {
    if (event.key === 'Escape') {
        const dropdownMenu = document.getElementById('dropdownMenu');
        if (dropdownMenu && dropdownMenu.classList.contains('show')) {
            dropdownMenu.classList.remove('show');
            const arrow = document.querySelector('.dropdown-arrow');
            if (arrow) {
                arrow.style.transform = 'rotate(0deg)';
            }
        }
    }
});

// ==================== 视频互动功能 ====================

// 点赞
function likeVideo(videoId, button) {
    fetch(`/api/like/${videoId}`, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        }
    })
    .then(response => response.json())
    .then(data => {
        if (data.liked) {
            button.classList.add('liked');
            button.innerHTML = `<i class="fas fa-heart"></i><span>${data.like_count}</span>`;
        } else {
            button.classList.remove('liked');
            button.innerHTML = `<i class="far fa-heart"></i><span>${data.like_count}</span>`;
        }
        // 更新通知
        updateUnreadNotifications();
    })
    .catch(error => {
        console.error('Error:', error);
        alert('操作失败，请稍后重试');
    });
}

// 收藏
function favoriteVideo(videoId, button) {
    fetch(`/api/favorite/${videoId}`, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        }
    })
    .then(response => response.json())
    .then(data => {
        if (data.favorited) {
            button.classList.add('favorited');
            button.innerHTML = `<i class="fas fa-star"></i><span>${data.favorite_count}</span>`;
        } else {
            button.classList.remove('favorited');
            button.innerHTML = `<i class="far fa-star"></i><span>${data.favorite_count}</span>`;
        }
        // 更新通知
        updateUnreadNotifications();
    })
    .catch(error => {
        console.error('Error:', error);
        alert('操作失败，请稍后重试');
    });
}

// 发表评论
function submitComment(videoId) {
    const textarea = document.getElementById('comment-content');
    const content = textarea.value.trim();
    
    if (!content) {
        alert('评论内容不能为空');
        return;
    }
    
    fetch(`/api/comment/${videoId}`, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({ content: content })
    })
    .then(response => response.json())
    .then(data => {
        // 添加新评论到列表
        const commentsList = document.getElementById('comments-list');
        const now = new Date();
        const timeStr = now.getFullYear() + '-' + 
                       String(now.getMonth() + 1).padStart(2, '0') + '-' + 
                       String(now.getDate()).padStart(2, '0') + ' ' + 
                       String(now.getHours()).padStart(2, '0') + ':' + 
                       String(now.getMinutes()).padStart(2, '0');
        
        const commentHtml = `
            <div class="comment-item">
                <div class="comment-header">
                    <img src="/uploads/avatars/${currentUserAvatar}" alt="avatar" class="comment-avatar">
                    <span class="comment-author">${data.username}</span>
                    <span class="comment-time">刚刚</span>
                </div>
                <div class="comment-content">${escapeHtml(data.content)}</div>
            </div>
        `;
        commentsList.insertAdjacentHTML('afterbegin', commentHtml);
        textarea.value = '';
        
        // 更新评论计数
        const commentCount = document.querySelector('.video-stats span:nth-child(4)');
        if (commentCount) {
            const count = parseInt(commentCount.textContent) + 1;
            commentCount.innerHTML = `<i class="fas fa-comment"></i> ${count}评论`;
        }
        
        // 更新通知
        updateUnreadNotifications();
    })
    .catch(error => {
        console.error('Error:', error);
        alert('评论发表失败，请稍后重试');
    });
}

// ==================== 好友功能 ====================

// 发送好友请求
function sendFriendRequest(userId) {
    fetch(`/api/friend/request/${userId}`, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        }
    })
    .then(response => response.json())
    .then(data => {
        if (data.success) {
            alert(data.message);
            location.reload();
        } else {
            alert(data.error || '操作失败');
        }
    })
    .catch(error => {
        console.error('Error:', error);
        alert('操作失败，请稍后重试');
    });
}

// 接受好友请求
function acceptFriendRequest(userId) {
    fetch(`/api/friend/accept/${userId}`, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        }
    })
    .then(response => response.json())
    .then(data => {
        if (data.success) {
            alert(data.message);
            location.reload();
        } else {
            alert(data.error || '操作失败');
        }
    })
    .catch(error => {
        console.error('Error:', error);
        alert('操作失败，请稍后重试');
    });
}

// 拒绝好友请求
function rejectFriendRequest(userId) {
    fetch(`/api/friend/reject/${userId}`, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        }
    })
    .then(response => response.json())
    .then(data => {
        if (data.success) {
            alert(data.message);
            location.reload();
        } else {
            alert(data.error || '操作失败');
        }
    })
    .catch(error => {
        console.error('Error:', error);
        alert('操作失败，请稍后重试');
    });
}

// 删除好友
function removeFriend(userId) {
    if (!confirm('确定要删除这个好友吗？')) {
        return;
    }
    
    fetch(`/api/friend/remove/${userId}`, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        }
    })
    .then(response => response.json())
    .then(data => {
        if (data.success) {
            alert(data.message);
            location.reload();
        }
    })
    .catch(error => {
        console.error('Error:', error);
        alert('操作失败，请稍后重试');
    });
}

// ==================== 聊天功能 ====================

// 打开聊天
function openChat(userId) {
    window.location.href = `/chat/${userId}`;
}

// 发送消息
function sendMessage() {
    const input = document.getElementById('message-input');
    const content = input.value.trim();
    const receiverId = document.getElementById('chat-receiver-id')?.value;
    
    if (!content || !receiverId) return;
    
    fetch('/api/message/send', {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({
            receiver_id: parseInt(receiverId),
            content: content
        })
    })
    .then(response => response.json())
    .then(data => {
        if (data.success) {
            // 清空输入框
            input.value = '';
            
            // 添加消息到聊天区域
            const messagesContainer = document.getElementById('chat-messages');
            const now = new Date();
            const timeStr = now.getHours().toString().padStart(2, '0') + ':' + 
                           now.getMinutes().toString().padStart(2, '0');
            
            const messageHtml = `
                <div class="message-item sent">
                    <img src="/uploads/avatars/${currentUserAvatar}" class="message-avatar">
                    <div class="message-content">
                        <div class="message-text">${escapeHtml(content)}</div>
                        <div class="message-time">${timeStr}</div>
                    </div>
                </div>
            `;
            messagesContainer.insertAdjacentHTML('beforeend', messageHtml);
            messagesContainer.scrollTop = messagesContainer.scrollHeight;
            
            // 更新消息计数
            updateUnreadMessages();
        } else {
            alert(data.error || '发送失败');
        }
    })
    .catch(error => {
        console.error('Error:', error);
        alert('发送失败，请稍后重试');
    });
}

// 按回车发送消息
function handleMessageKeyPress(event) {
    if (event.key === 'Enter' && !event.shiftKey) {
        event.preventDefault();
        sendMessage();
    }
}

// ==================== 分享功能 ====================

// 打开分享模态框
function openShareModal(videoId, videoTitle) {
    document.getElementById('share-video-id').value = videoId;
    document.getElementById('share-modal').classList.add('active');
}

// 关闭分享模态框
function closeShareModal() {
    document.getElementById('share-modal').classList.remove('active');
}

// 选择好友分享
function selectFriendForShare(friendId, friendName) {
    const videoId = document.getElementById('share-video-id').value;
    shareVideo(videoId, friendId, friendName);
}

// 分享视频
function shareVideo(videoId, friendId, friendName) {
    fetch(`/api/share/${videoId}/${friendId}`, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        }
    })
    .then(response => response.json())
    .then(data => {
        if (data.success) {
            alert(`已分享给 ${friendName}`);
            closeShareModal();
        } else {
            alert(data.error || '分享失败');
        }
    })
    .catch(error => {
        console.error('Error:', error);
        alert('分享失败，请稍后重试');
    });
}

// ==================== 通知功能 ====================

// 更新未读通知数量
function updateUnreadNotifications() {
    fetch('/api/notifications/unread')
        .then(response => response.json())
        .then(data => {
            const badge = document.getElementById('notification-badge');
            if (badge) {
                if (data.count > 0) {
                    badge.textContent = data.count;
                    badge.style.display = 'inline';
                } else {
                    badge.style.display = 'none';
                }
            }
        })
        .catch(error => console.error('Error updating notifications:', error));
}

// 更新未读消息数量
function updateUnreadMessages() {
    fetch('/api/messages/unread')
        .then(response => response.json())
        .then(data => {
            const badge = document.getElementById('message-badge');
            if (badge) {
                if (data.count > 0) {
                    badge.textContent = data.count;
                    badge.style.display = 'inline';
                } else {
                    badge.style.display = 'none';
                }
            }
        })
        .catch(error => console.error('Error updating messages:', error));
}

// ==================== 搜索功能 ====================

// 搜索防抖
function debounceSearch(input, callback) {
    clearTimeout(searchTimeout);
    searchTimeout = setTimeout(() => {
        callback(input.value);
    }, 500);
}

// ==================== 文件上传预览 ====================

// 预览封面
function previewCover(input) {
    if (input.files && input.files[0]) {
        const reader = new FileReader();
        reader.onload = function(e) {
            const preview = document.getElementById('cover-preview');
            preview.src = e.target.result;
            preview.style.display = 'block';
        };
        reader.readAsDataURL(input.files[0]);
    }
}

// 预览头像
function previewAvatar(input) {
    if (input.files && input.files[0]) {
        const reader = new FileReader();
        reader.onload = function(e) {
            document.getElementById('avatar-preview').src = e.target.result;
        };
        reader.readAsDataURL(input.files[0]);
    }
}

// ==================== 表单验证 ====================

// 验证上传表单
function validateUploadForm() {
    const title = document.getElementById('title').value.trim();
    const video = document.getElementById('video').files[0];
    
    if (!title) {
        alert('请输入视频标题');
        return false;
    }
    
    if (!video) {
        alert('请选择视频文件');
        return false;
    }
    
    // 检查视频文件大小（最大500MB）
    if (video.size > 500 * 1024 * 1024) {
        alert('视频文件不能超过500MB');
        return false;
    }
    
    return true;
}

// ==================== 标签页切换 ====================

// 切换标签页
function switchTab(tabName) {
    // 隐藏所有标签页
    document.querySelectorAll('.tab-panel').forEach(panel => {
        panel.classList.remove('active');
    });
    
    // 取消所有标签按钮的激活状态
    document.querySelectorAll('.tab-btn').forEach(btn => {
        btn.classList.remove('active');
    });
    
    // 显示选中的标签页
    document.getElementById(`tab-${tabName}`).classList.add('active');
    
    // 激活对应的按钮
    event.target.classList.add('active');
}

// ==================== 定时更新 ====================

// 定时更新未读通知（每30秒）
setInterval(() => {
    if (document.getElementById('notification-badge')) {
        updateUnreadNotifications();
        updateUnreadMessages();
    }
}, 30000);