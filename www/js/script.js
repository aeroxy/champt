var socket = io.connect('http://localhost');
$(document).ready(function(){
	function resizeText() {
		var windowHeight = $(window).height();
		var windowWidth = $(window).width();
		$("footer").css({
			'padding-top': windowHeight * 0.15 * 0.1 + 'px',
		});
	};
	resizeText();
	$(window).resize(function(){
		resizeText();
	});
	navigator.geolocation.watchPosition(function(position){
		var latitude = position.coords.latitude;
		var longitude = position.coords.longitude;
		console.log(latitude,longitude);
		socket.emit('addToMap',latitude,longitude);
		socket.on('meOnMap',function(latitude,longitude){
			var gLatLng = new google.maps.LatLng(latitude, longitude);
			function drawMap(){
				var mapOptions = {
					center: gLatLng,
					zoom: 18,
					mapTypeId: google.maps.MapTypeId.ROADMAP,
					draggable: false,
					zoomControl: false,
					scrollwheel: false,
					disableDoubleClickZoom: true,
					disableDefaultUI: true,
				};
				var map = new google.maps.Map(document.getElementById("map-canvas"),mapOptions);
				return map;
			}
			var map = drawMap();
			function drawUser(){
				var nameLatLng = new google.maps.LatLng(latitude - 0.00004, longitude);
				var userLocation = {
					path: google.maps.SymbolPath.CIRCLE,
					strokeColor: "#4c8f2a",
					strokeWeight: 2,
					fillColor: "#a2e346",
					fillOpacity: 1,
					scale: 6,
				};
				var userName = new MapLabel({
					text: 'Anonymous User',
					position: nameLatLng,
					map: map,
					fontSize: 14,
					fontColor: "#ffffff",
					strokeWeight: 4,
					strokeColor: "4c8f2a",
					fontFamily: "Avenir",
				});
				var drawCircle = new google.maps.Marker({
					position: gLatLng,
					icon: userLocation,
					map: map,
				});
				return userName;
			};
			var userName = drawUser();
			function updateUser(){
				var infoBubble = new InfoBubble({
					position: gLatLng,
					maxWidth: 300,
					backgroundColor: 'rgba(255,255,255,0.8)',
					backgroundClassName: 'bubble',
					hideCloseButton: true,
					shadowStyle: 0,
				});
				$("#update").click(function(){
					var status = $("#status-update").val();
					userName.set('text', $("#userName").val());
					infoBubble.addTab($("#userName").val() + ':', status);
					infoBubble.open(map);
				});
			};
			updateUser();
			socket.on('otherOnMap',function(latitude,longitude){
				var gLatLng = new google.maps.LatLng(latitude, longitude);
				function drawOtherUsers(){
					var nameLatLng = new google.maps.LatLng(latitude - 0.00004, longitude);
					var userLocation = {
						path: google.maps.SymbolPath.CIRCLE,
						strokeColor: "#ff0000",
						strokeWeight: 2,
						fillColor: "#ffffff",
						fillOpacity: 1,
						scale: 6,
					};
					var userName = new MapLabel({
						text: 'Anonymous User',
						position: nameLatLng,
						map: map,
						fontSize: 14,
						fontColor: "#ffffff",
						strokeWeight: 4,
						strokeColor: "#ff0000",
						fontFamily: "Avenir",
					});
					var drawCircle = new google.maps.Marker({
						position: gLatLng,
						icon: userLocation,
						map: map,
					});
					return userName;
				};
				var userName = drawOtherUsers();
				updateUser();
			});
		});
	});
});