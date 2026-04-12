import os
import uuid
from datetime import datetime, timedelta
from functools import wraps
from werkzeug.utils import secure_filename
from flask import Flask, render_template, request, redirect, url_for, flash, jsonify, send_from_directory, session
from flask_login import LoginManager, login_user, logout_user, login_required, current_user
from sqlalchemy.exc import IntegrityError
from database import db, User, Video, Like, Favorite, Comment, Friendship, Message, Notification, Danmaku, OAuthAccount
import requests
import secrets
from urllib.parse import urlencode

app = Flask(__name__)
app.config.from_object('config.Config')
app.secret_key = 'your-secret-key-here-change-in-production'  # 需要设置 secret_key 用于 session

# OAuth2 配置
OAUTH_CONFIG = {
    'ljworld': {
        'name': '清凉视频',
        'client_id': 'wXLR0WwYesg5tTXxGft10CVu',
        'client_secret': 'Qiq9Ge5CeJTyL-B5dpA9tIL6ky72JoWfUJaE7YY7',
        'authorize_url': 'http://localhost:8080/oauth/authorize',
        'token_url': 'http://localhost:8080/oauth/token',
        'userinfo_url': 'http://localhost:8080/api/oauth/userinfo',
        'redirect_uri': 'http://localhost:3000/callback',
        'scope': 'userinfo'
    }
}

# 初始化数据库
db.init_app(app)

# 初始化登录管理器
login_manager = LoginManager()
login_manager.init_app(app)
login_manager.login_view = 'login'

# 确保上传目录存在
os.makedirs(app.config['UPLOAD_FOLDER'], exist_ok=True)
os.makedirs(os.path.join(app.config['UPLOAD_FOLDER'], 'videos'), exist_ok=True)
os.makedirs(os.path.join(app.config['UPLOAD_FOLDER'], 'covers'), exist_ok=True)
os.makedirs(os.path.join(app.config['UPLOAD_FOLDER'], 'avatars'), exist_ok=True)

# ==================== 辅助函数 ====================

def format_number(num):
    """格式化数字显示（例如：12000 -> 1.2万）"""
    if num >= 10000:
        return f"{num/10000:.1f}万"
    return str(num)

def time_ago(dt):
    """计算时间差（例如：3小时前）"""
    if not dt:
        return "未知"
    now = datetime.utcnow()
    diff = now - dt
    
    seconds = diff.total_seconds()
    if seconds < 60:
        return "刚刚"
    elif seconds < 3600:
        return f"{int(seconds // 60)}分钟前"
    elif seconds < 86400:
        return f"{int(seconds // 3600)}小时前"
    elif seconds < 2592000:
        return f"{int(seconds // 86400)}天前"
    elif seconds < 31536000:
        return f"{int(seconds // 2592000)}月前"
    else:
        return f"{int(seconds // 31536000)}年前"

# 注册模板全局函数
@app.context_processor
def utility_processor():
    return dict(format_number=format_number, time_ago=time_ago)

def allowed_video_file(filename):
    """检查视频文件格式"""
    return '.' in filename and filename.rsplit('.', 1)[1].lower() in app.config['ALLOWED_VIDEO_EXTENSIONS']

def allowed_image_file(filename):
    """检查图片文件格式"""
    return '.' in filename and filename.rsplit('.', 1)[1].lower() in app.config['ALLOWED_IMAGE_EXTENSIONS']

@login_manager.user_loader
def load_user(user_id):
    """加载用户"""
    return db.session.get(User, int(user_id))

# ==================== 首页 ====================

@app.route('/')
@app.route('/index')
def index():
    """首页 - 视频列表"""
    page = request.args.get('page', 1, type=int)
    search = request.args.get('search', '')
    category = request.args.get('category', '')
    feed = request.args.get('feed', '')
    
    # 基础查询
    query = Video.query
    
    # 搜索过滤
    if search:
        query = query.filter(
            Video.title.contains(search) | 
            Video.description.contains(search)
        )
    
    # 分类过滤（可以根据需要扩展）
    if category:
        pass
    
    # 排序
    if feed == 'hot':
        query = query.order_by(Video.views.desc())
    elif feed == 'follow' and current_user.is_authenticated:
        friends = current_user.get_friends()
        friend_ids = [f.id for f in friends]
        if friend_ids:
            query = query.filter(Video.user_id.in_(friend_ids))
        query = query.order_by(Video.created_at.desc())
    else:
        query = query.order_by(Video.created_at.desc())
    
    # 分页
    videos = query.paginate(page=page, per_page=20, error_out=False)
    
    banner = {
        'image': 'https://via.placeholder.com/1200x100/fb7299/ffffff?text=轻视频+欢迎你',
        'url': '#',
        'title': '轻视频 - 年轻人的视频社区'
    }
    
    return render_template('index.html', 
                         videos=videos, 
                         search=search,
                         category=category,
                         feed=feed,
                         banner=banner)

# ==================== 用户认证 ====================

@app.route('/register', methods=['GET', 'POST'])
def register():
    """用户注册"""
    if request.method == 'POST':
        username = request.form.get('username')
        email = request.form.get('email')
        password = request.form.get('password')
        
        if User.query.filter_by(username=username).first():
            flash('用户名已存在', 'error')
            return redirect(url_for('register'))
        
        if User.query.filter_by(email=email).first():
            flash('邮箱已注册', 'error')
            return redirect(url_for('register'))
        
        user = User(username=username, email=email)
        user.set_password(password)
        db.session.add(user)
        
        try:
            db.session.commit()
            flash('注册成功，请登录', 'success')
            return redirect(url_for('login'))
        except IntegrityError:
            db.session.rollback()
            flash('注册失败，邮箱可能已被使用', 'error')
            return redirect(url_for('register'))
    
    return render_template('register.html')

@app.route('/login', methods=['GET', 'POST'])
def login():
    """用户登录"""
    if request.method == 'POST':
        username = request.form.get('username')
        password = request.form.get('password')
        remember = True if request.form.get('remember') else False
        
        user = User.query.filter_by(username=username).first()
        
        if not user or not user.check_password(password):
            flash('用户名或密码错误', 'error')
            return redirect(url_for('login'))
        
        login_user(user, remember=remember)
        
        next_page = request.args.get('next')
        if next_page:
            return redirect(next_page)
        return redirect(url_for('index'))
    
    return render_template('login.html')

@app.route('/logout')
@login_required
def logout():
    """退出登录"""
    logout_user()
    return redirect(url_for('index'))

# ==================== 第三方登录（凌界 OAuth2） ====================

@app.route('/oauth/login/<provider>')
def oauth_login(provider):
    """跳转到第三方授权页"""
    if provider not in OAUTH_CONFIG:
        flash('不支持的登录方式', 'error')
        return redirect(url_for('login'))
    
    config = OAUTH_CONFIG[provider]
    
    state = secrets.token_urlsafe(32)
    session['oauth_state'] = state
    
    params = {
        'response_type': 'code',
        'client_id': config['client_id'],
        'redirect_uri': config['redirect_uri'],
        'state': state,
        'scope': config.get('scope', 'userinfo')
    }
    auth_url = f"{config['authorize_url']}?{urlencode(params)}"
    
    return redirect(auth_url)


@app.route('/callback')
def oauth_callback():
    """OAuth 回调处理"""
    provider = 'ljworld'
    
    if provider not in OAUTH_CONFIG:
        flash('不支持的登录方式', 'error')
        return redirect(url_for('login'))
    
    config = OAUTH_CONFIG[provider]
    
    code = request.args.get('code')
    state = request.args.get('state')
    error = request.args.get('error')
    
    if error:
        error_desc = request.args.get('error_description', '授权失败')
        flash(f'登录失败：{error_desc}', 'error')
        return redirect(url_for('login'))
    
    session_state = session.pop('oauth_state', None)
    if not state or state != session_state:
        flash('登录失败：状态验证失败', 'error')
        return redirect(url_for('login'))
    
    if not code:
        flash('登录失败：未获取到授权码', 'error')
        return redirect(url_for('login'))
    
    try:
        token_data = {
            'grant_type': 'authorization_code',
            'code': code,
            'redirect_uri': config['redirect_uri'],
            'client_id': config['client_id'],
            'client_secret': config['client_secret']
        }
        
        token_response = requests.post(
            config['token_url'],
            data=token_data,
            headers={'Content-Type': 'application/x-www-form-urlencoded'}
        )
        
        if token_response.status_code != 200:
            flash('获取令牌失败', 'error')
            return redirect(url_for('login'))
        
        token_info = token_response.json()
        access_token = token_info.get('access_token')
        
        if not access_token:
            flash('获取令牌失败', 'error')
            return redirect(url_for('login'))
        
        userinfo_response = requests.get(
            config['userinfo_url'],
            headers={'Authorization': f'Bearer {access_token}'}
        )
        
        if userinfo_response.status_code != 200:
            flash('获取用户信息失败', 'error')
            return redirect(url_for('login'))
        
        user_info = userinfo_response.json()
        provider_user_id = str(user_info.get('id'))
        
        oauth_account = OAuthAccount.query.filter_by(
            provider=provider,
            provider_user_id=provider_user_id
        ).first()
        
        if oauth_account:
            user = oauth_account.user
            login_user(user, remember=True)
            flash(f'欢迎回来，{user.username}！', 'success')
            return redirect(url_for('index'))
        
        email = user_info.get('email')
        if email:
            user = User.query.filter_by(email=email).first()
            if user:
                oauth_account = OAuthAccount(
                    user_id=user.id,
                    provider=provider,
                    provider_user_id=provider_user_id
                )
                db.session.add(oauth_account)
                db.session.commit()
                login_user(user, remember=True)
                flash(f'欢迎回来，{user.username}！已关联{config["name"]}', 'success')
                return redirect(url_for('index'))
        
        username = user_info.get('username') or user_info.get('nickname')
        original_username = username
        counter = 1
        while User.query.filter_by(username=username).first():
            username = f"{original_username}_{counter}"
            counter += 1
        
        new_user = User(
            username=username,
            email=user_info.get('email') or f"{provider_user_id}@oauth.ljworld.com",
            avatar=user_info.get('avatar', 'default_avatar.png'),
            bio=user_info.get('bio', f'通过{config["name"]}注册')
        )
        new_user.set_password(secrets.token_urlsafe(16))
        db.session.add(new_user)
        db.session.flush()
        
        oauth_account = OAuthAccount(
            user_id=new_user.id,
            provider=provider,
            provider_user_id=provider_user_id
        )
        db.session.add(oauth_account)
        db.session.commit()
        
        login_user(new_user, remember=True)
        flash(f'欢迎新用户 {new_user.username}！您已通过{config["name"]}登录', 'success')
        return redirect(url_for('index'))
        
    except Exception as e:
        db.session.rollback()
        flash(f'登录失败：{str(e)}', 'error')
        return redirect(url_for('login'))

# ==================== 视频上传 ====================

@app.route('/upload', methods=['GET', 'POST'])
@login_required
def upload():
    """视频投稿"""
    if request.method == 'POST':
        title = request.form.get('title')
        description = request.form.get('description')
        video_file = request.files.get('video')
        cover_file = request.files.get('cover')
        
        if not title or not video_file:
            flash('标题和视频文件不能为空', 'error')
            return redirect(url_for('upload'))
        
        if video_file and allowed_video_file(video_file.filename):
            video_filename = secure_filename(video_file.filename)
            video_filename = f"{uuid.uuid4()}_{video_filename}"
            video_path = os.path.join(app.config['UPLOAD_FOLDER'], 'videos', video_filename)
            video_file.save(video_path)
        else:
            flash('不支持的视频格式', 'error')
            return redirect(url_for('upload'))
        
        cover_filename = 'default_cover.jpg'
        if cover_file and allowed_image_file(cover_file.filename):
            cover_filename = secure_filename(cover_file.filename)
            cover_filename = f"{uuid.uuid4()}_{cover_filename}"
            cover_path = os.path.join(app.config['UPLOAD_FOLDER'], 'covers', cover_filename)
            cover_file.save(cover_path)
        
        video = Video(
            title=title,
            description=description,
            video_file=video_filename,
            cover_image=cover_filename,
            user_id=current_user.id
        )
        db.session.add(video)
        db.session.commit()
        
        flash('视频上传成功', 'success')
        return redirect(url_for('video', video_id=video.id))
    
    return render_template('upload.html')

# ==================== 视频详情 ====================

@app.route('/video/<int:video_id>')
def video(video_id):
    """视频详情页"""
    video = Video.query.get_or_404(video_id)
    video.views += 1
    db.session.commit()
    
    user_liked = False
    user_favorited = False
    if current_user.is_authenticated:
        user_liked = Like.query.filter_by(user_id=current_user.id, video_id=video_id).first() is not None
        user_favorited = Favorite.query.filter_by(user_id=current_user.id, video_id=video_id).first() is not None
    
    comments = Comment.query.filter_by(video_id=video_id).order_by(Comment.created_at.desc()).all()
    
    friends = []
    if current_user.is_authenticated:
        friends = current_user.get_friends()
    
    related_videos = Video.query.filter(
        Video.user_id == video.user_id,
        Video.id != video.id
    ).order_by(Video.created_at.desc()).limit(10).all()
    
    return render_template('video.html', 
                         video=video, 
                         user_liked=user_liked, 
                         user_favorited=user_favorited,
                         comments=comments,
                         friends=friends,
                         related_videos=related_videos)

# ==================== 删除视频 ====================

@app.route('/video/<int:video_id>/delete', methods=['POST'])
@login_required
def delete_video(video_id):
    """删除视频"""
    video = Video.query.get_or_404(video_id)
    
    if video.user_id != current_user.id:
        flash('你没有权限删除这个视频', 'error')
        return redirect(url_for('index'))
    
    try:
        video_path = os.path.join(app.config['UPLOAD_FOLDER'], 'videos', video.video_file)
        if os.path.exists(video_path):
            os.remove(video_path)
        
        if video.cover_image != 'default_cover.jpg':
            cover_path = os.path.join(app.config['UPLOAD_FOLDER'], 'covers', video.cover_image)
            if os.path.exists(cover_path):
                os.remove(cover_path)
        
        Notification.query.filter(
            (Notification.type.in_(['like', 'favorite', 'comment'])) &
            (Notification.related_id == video_id)
        ).delete()
        
        Message.query.filter_by(video_id=video_id).delete()
        
        db.session.delete(video)
        db.session.commit()
        
        flash('视频删除成功', 'success')
    except Exception as e:
        db.session.rollback()
        flash('删除失败：' + str(e), 'error')
    
    return redirect(url_for('profile'))

# ==================== 文件访问 ====================

@app.route('/uploads/videos/<filename>')
def uploaded_video(filename):
    return send_from_directory(os.path.join(app.config['UPLOAD_FOLDER'], 'videos'), filename)

@app.route('/uploads/covers/<filename>')
def uploaded_cover(filename):
    return send_from_directory(os.path.join(app.config['UPLOAD_FOLDER'], 'covers'), filename)

@app.route('/uploads/avatars/<filename>')
def uploaded_avatar(filename):
    return send_from_directory(os.path.join(app.config['UPLOAD_FOLDER'], 'avatars'), filename)

# ==================== 个人中心 ====================

@app.route('/profile')
@login_required
def profile():
    user_videos = Video.query.filter_by(user_id=current_user.id).order_by(Video.created_at.desc()).all()
    favorite_videos = [fav.video for fav in Favorite.query.filter_by(user_id=current_user.id).order_by(Favorite.created_at.desc()).all()]
    friends = current_user.get_friends()
    pending_requests = current_user.get_pending_requests()
    unread_notifications = Notification.query.filter_by(user_id=current_user.id, is_read=False).count()
    
    return render_template('profile.html', 
                         user_videos=user_videos, 
                         favorite_videos=favorite_videos,
                         friends=friends,
                         pending_requests=pending_requests,
                         unread_notifications=unread_notifications)

@app.route('/profile/avatar', methods=['POST'])
@login_required
def update_avatar():
    avatar_file = request.files.get('avatar')
    
    if not avatar_file or not allowed_image_file(avatar_file.filename):
        flash('请选择有效的图片文件', 'error')
        return redirect(url_for('profile'))
    
    if current_user.avatar and current_user.avatar != 'default_avatar.png':
        old_avatar_path = os.path.join(app.config['UPLOAD_FOLDER'], 'avatars', current_user.avatar)
        if os.path.exists(old_avatar_path):
            os.remove(old_avatar_path)
    
    avatar_filename = secure_filename(avatar_file.filename)
    avatar_filename = f"{uuid.uuid4()}_{avatar_filename}"
    avatar_path = os.path.join(app.config['UPLOAD_FOLDER'], 'avatars', avatar_filename)
    avatar_file.save(avatar_path)
    
    current_user.avatar = avatar_filename
    db.session.commit()
    
    flash('头像修改成功', 'success')
    return redirect(url_for('profile'))

@app.route('/profile/bio', methods=['POST'])
@login_required
def update_bio():
    bio = request.form.get('bio', '')
    current_user.bio = bio[:500]
    db.session.commit()
    flash('个人简介更新成功', 'success')
    return redirect(url_for('profile'))

# ==================== 点赞/收藏/评论 ====================

@app.route('/api/like/<int:video_id>', methods=['POST'])
@login_required
def like_video(video_id):
    video = Video.query.get_or_404(video_id)
    like = Like.query.filter_by(user_id=current_user.id, video_id=video_id).first()
    
    if like:
        db.session.delete(like)
        liked = False
    else:
        like = Like(user_id=current_user.id, video_id=video_id)
        db.session.add(like)
        liked = True
        
        if video.user_id != current_user.id:
            notification = Notification(
                user_id=video.user_id,
                type='like',
                content=f'{current_user.username} 点赞了你的视频《{video.title}》',
                related_id=video_id
            )
            db.session.add(notification)
    
    db.session.commit()
    
    return jsonify({
        'liked': liked,
        'like_count': video.like_count
    })

@app.route('/api/favorite/<int:video_id>', methods=['POST'])
@login_required
def favorite_video(video_id):
    video = Video.query.get_or_404(video_id)
    favorite = Favorite.query.filter_by(user_id=current_user.id, video_id=video_id).first()
    
    if favorite:
        db.session.delete(favorite)
        favorited = False
    else:
        favorite = Favorite(user_id=current_user.id, video_id=video_id)
        db.session.add(favorite)
        favorited = True
        
        if video.user_id != current_user.id:
            notification = Notification(
                user_id=video.user_id,
                type='favorite',
                content=f'{current_user.username} 收藏了你的视频《{video.title}》',
                related_id=video_id
            )
            db.session.add(notification)
    
    db.session.commit()
    
    return jsonify({
        'favorited': favorited,
        'favorite_count': video.favorite_count
    })

@app.route('/api/comment/<int:video_id>', methods=['POST'])
@login_required
def add_comment(video_id):
    data = request.get_json()
    content = data.get('content')
    video = Video.query.get_or_404(video_id)
    
    if not content:
        return jsonify({'error': '评论内容不能为空'}), 400
    
    comment = Comment(
        content=content,
        user_id=current_user.id,
        video_id=video_id
    )
    db.session.add(comment)
    
    if video.user_id != current_user.id:
        notification = Notification(
            user_id=video.user_id,
            type='comment',
            content=f'{current_user.username} 评论了你的视频《{video.title}》',
            related_id=video_id
        )
        db.session.add(notification)
    
    db.session.commit()
    
    return jsonify(comment.to_dict())

# ==================== 好友系统 ====================

@app.route('/friends')
@login_required
def friends_page():
    friends = current_user.get_friends()
    pending_requests = current_user.get_pending_requests()
    
    search = request.args.get('search', '')
    search_results = []
    if search:
        search_results = User.query.filter(
            User.username.contains(search),
            User.id != current_user.id
        ).limit(10).all()
    
    return render_template('friends.html', 
                         friends=friends, 
                         pending_requests=pending_requests,
                         search_results=search_results,
                         search=search)

@app.route('/api/friend/request/<int:user_id>', methods=['POST'])
@login_required
def send_friend_request(user_id):
    if user_id == current_user.id:
        return jsonify({'error': '不能添加自己为好友'}), 400
    
    user = User.query.get_or_404(user_id)
    
    if current_user.is_friend(user):
        return jsonify({'error': '已经是好友了'}), 400
    
    if current_user.add_friend(user):
        return jsonify({'success': True, 'message': '好友请求已发送'})
    
    return jsonify({'error': '操作失败'}), 400

@app.route('/api/friend/accept/<int:user_id>', methods=['POST'])
@login_required
def accept_friend_request(user_id):
    user = User.query.get_or_404(user_id)
    
    if current_user.accept_friend(user):
        return jsonify({'success': True, 'message': '已接受好友请求'})
    
    return jsonify({'error': '操作失败'}), 400

@app.route('/api/friend/reject/<int:user_id>', methods=['POST'])
@login_required
def reject_friend_request(user_id):
    user = User.query.get_or_404(user_id)
    
    if current_user.reject_friend(user):
        return jsonify({'success': True, 'message': '已拒绝好友请求'})
    
    return jsonify({'error': '操作失败'}), 400

@app.route('/api/friend/remove/<int:user_id>', methods=['POST'])
@login_required
def remove_friend(user_id):
    user = User.query.get_or_404(user_id)
    current_user.remove_friend(user)
    return jsonify({'success': True, 'message': '已删除好友'})

# ==================== 聊天系统 ====================

@app.route('/chat')
@app.route('/chat/<int:user_id>')
@login_required
def chat(user_id=None):
    friend = None
    messages = []
    
    if user_id:
        friend = User.query.get_or_404(user_id)
        if not current_user.is_friend(friend):
            flash('你们还不是好友', 'error')
            return redirect(url_for('friends_page'))
        
        messages = Message.query.filter(
            ((Message.sender_id == current_user.id) & (Message.receiver_id == user_id)) |
            ((Message.sender_id == user_id) & (Message.receiver_id == current_user.id))
        ).order_by(Message.created_at.asc()).all()
        
        Message.query.filter_by(sender_id=user_id, receiver_id=current_user.id, is_read=False).update({'is_read': True})
        db.session.commit()
    
    conversations = current_user.get_conversations()
    
    return render_template('chat.html', 
                         friend=friend, 
                         messages=messages, 
                         conversations=conversations)

@app.route('/api/message/send', methods=['POST'])
@login_required
def send_message():
    data = request.get_json()
    receiver_id = data.get('receiver_id')
    content = data.get('content', '').strip()
    message_type = data.get('type', 'text')
    video_id = data.get('video_id')
    
    receiver = User.query.get_or_404(receiver_id)
    
    if not current_user.is_friend(receiver):
        return jsonify({'error': '你们还不是好友'}), 400
    
    if not content and message_type == 'text':
        return jsonify({'error': '消息内容不能为空'}), 400
    
    message = current_user.send_message(receiver, content, message_type, video_id)
    
    return jsonify({
        'success': True,
        'message': {
            'id': message.id,
            'content': message.content,
            'created_at': message.created_at.strftime('%Y-%m-%d %H:%M:%S'),
            'message_type': message.message_type,
            'video_id': message.video_id
        }
    })

@app.route('/api/share/<int:video_id>/<int:friend_id>', methods=['POST'])
@login_required
def share_video(video_id, friend_id):
    video = Video.query.get_or_404(video_id)
    friend = User.query.get_or_404(friend_id)
    
    if not current_user.is_friend(friend):
        return jsonify({'error': '你们还不是好友'}), 400
    
    share_message = f"分享了视频：《{video.title}》"
    current_user.send_message(friend, share_message, 'video_share', video_id)
    
    return jsonify({'success': True, 'message': '分享成功'})

# ==================== 弹幕系统 ====================

@app.route('/api/danmaku/<int:video_id>')
def get_danmaku(video_id):
    video = Video.query.get_or_404(video_id)
    danmakus = Danmaku.query.filter_by(video_id=video_id).order_by(Danmaku.time.asc()).all()
    return jsonify([d.to_dict() for d in danmakus])

@app.route('/api/danmaku/<int:video_id>', methods=['POST'])
def send_danmaku(video_id):
    video = Video.query.get_or_404(video_id)
    data = request.get_json()
    
    content = data.get('content', '').strip()
    if not content:
        return jsonify({'error': '弹幕内容不能为空'}), 400
    
    if len(content) > 100:
        return jsonify({'error': '弹幕不能超过100个字符'}), 400
    
    time = data.get('time', 0)
    
    danmaku = Danmaku(
        video_id=video_id,
        user_id=current_user.id if current_user.is_authenticated else None,
        content=content,
        time=time
    )
    
    db.session.add(danmaku)
    video.danmaku_count += 1
    db.session.commit()
    
    return jsonify(danmaku.to_dict())

# ==================== 通知系统 ====================

@app.route('/notifications')
@login_required
def notifications():
    page = request.args.get('page', 1, type=int)
    notifications = Notification.query.filter_by(user_id=current_user.id)\
        .order_by(Notification.created_at.desc())\
        .paginate(page=page, per_page=20, error_out=False)
    
    Notification.query.filter_by(user_id=current_user.id, is_read=False).update({'is_read': True})
    db.session.commit()
    
    return render_template('notifications.html', notifications=notifications)

@app.route('/api/notifications/unread')
@login_required
def get_unread_notifications():
    count = Notification.query.filter_by(user_id=current_user.id, is_read=False).count()
    return jsonify({'count': count})

@app.route('/api/messages/unread')
@login_required
def get_unread_messages():
    count = Message.query.filter_by(receiver_id=current_user.id, is_read=False).count()
    return jsonify({'count': count})

# ==================== 数据库初始化 ====================

@app.cli.command('init-db')
def init_db():
    """初始化数据库"""
    db.create_all()
    print('✅ 数据库初始化完成')

if __name__ == '__main__':
    with app.app_context():
        db.create_all()
    app.run(debug=True, host='0.0.0.0', port=3000)